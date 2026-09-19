package world.dot.terminal;

import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.io.*;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.security.*;
import java.security.cert.*;
import java.security.spec.ECGenParameterSpec;
import java.util.Date;
import javax.net.ssl.*;
import javax.security.auth.x500.X500Principal;
import org.json.JSONObject;

/** Private key stays in Android Keystore. Network addresses never establish trust. */
final class DeviceLink {
  private static final String ALIAS = "dot-device-identity-v2";
  private final Context context;
  private final KeyStore keys;
  final String nodeId;

  DeviceLink(Context context) throws Exception {
    this.context = context;
    keys = KeyStore.getInstance("AndroidKeyStore");
    keys.load(null);
    if (!keys.containsAlias(ALIAS)) {
      KeyPairGenerator generator =
          KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore");
      generator.initialize(
          new KeyGenParameterSpec.Builder(
                  ALIAS, KeyProperties.PURPOSE_SIGN | KeyProperties.PURPOSE_VERIFY)
              .setAlgorithmParameterSpec(new ECGenParameterSpec("secp256r1"))
              .setDigests(
                  // Conscrypt hashes TLS handshake data before asking Keystore to sign.
                  KeyProperties.DIGEST_NONE,
                  KeyProperties.DIGEST_SHA256,
                  KeyProperties.DIGEST_SHA384,
                  KeyProperties.DIGEST_SHA512)
              .setCertificateSubject(new X500Principal("CN=DOT Android device"))
              .setCertificateSerialNumber(java.math.BigInteger.ONE)
              .setCertificateNotBefore(new Date(System.currentTimeMillis() - 86400000L))
              .setCertificateNotAfter(new Date(System.currentTimeMillis() + 315360000000L))
              .build());
      generator.generateKeyPair();
    }
    java.security.cert.Certificate cert = keys.getCertificate(ALIAS);
    try (FileOutputStream out = context.openFileOutput("device-cert.der", Context.MODE_PRIVATE)) {
      out.write(cert.getEncoded());
    }
    StringBuilder id = new StringBuilder("dot:node:v1:");
    for (byte b : MessageDigest.getInstance("SHA-256").digest(cert.getPublicKey().getEncoded()))
      id.append(String.format(java.util.Locale.ROOT, "%02x", b & 255));
    nodeId = id.toString();
  }

  boolean paired() {
    return new File(context.getFilesDir(), "peer.json").isFile();
  }

  JSONObject call(JSONObject request) throws Exception {
    File file = new File(context.getFilesDir(), "peer.json");
    if (file.length() > 32768) throw new IOException("Pairing profile too large");
    JSONObject peer;
    try (InputStream in = new FileInputStream(file)) {
      byte[] data = new byte[(int) file.length()];
      new DataInputStream(in).readFully(data);
      peer = new JSONObject(new String(data, StandardCharsets.UTF_8));
    }
    return exchange(peer, request, false, 4000);
  }

  JSONObject invitation(String capsule) throws Exception {
    if (capsule.length() > 12000 || !capsule.startsWith("dot-pair:v1:"))
      throw new IOException("Not a DOT pairing invitation");
    byte[] json =
        Base64.decode(capsule.substring(12), Base64.URL_SAFE | Base64.NO_WRAP | Base64.NO_PADDING);
    return new JSONObject(NativeBridge.checkPair(new String(json, StandardCharsets.UTF_8), true));
  }

  private byte[] proofMessage(JSONObject invitation) throws Exception {
    String header =
        "DOT-PAIR-V1\0"
            + invitation.getString("token")
            + "\0"
            + invitation.getString("host")
            + "\0"
            + invitation.getInt("port")
            + "\0"
            + invitation.getInt("service_port")
            + "\0"
            + invitation.getLong("expires_at")
            + "\0"
            + invitation.getBoolean("terminal")
            + "\0"
            + invitation.getBoolean("clipboard_read")
            + "\0"
            + invitation.getBoolean("clipboard_write")
            + "\0";
    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    bytes.write(header.getBytes(StandardCharsets.UTF_8));
    bytes.write(
        MessageDigest.getInstance("SHA-256")
            .digest(Base64.decode(invitation.getString("certificate"), Base64.DEFAULT)));
    return bytes.toByteArray();
  }

  String confirmationCode(JSONObject invitation) throws Exception {
    MessageDigest digest = MessageDigest.getInstance("SHA-256");
    digest.update(proofMessage(invitation));
    digest.update(keys.getCertificate(ALIAS).getEncoded());
    byte[] hash = digest.digest();
    StringBuilder code = new StringBuilder();
    for (int i = 0; i < 8; i++)
      code.append(String.format(java.util.Locale.ROOT, "%02x", hash[i] & 255));
    return code.toString();
  }

  void pair(JSONObject invitation) throws Exception {
    byte[] cert = keys.getCertificate(ALIAS).getEncoded();
    Signature signer = Signature.getInstance("SHA256withECDSA");
    signer.initSign((PrivateKey) keys.getKey(ALIAS, null));
    signer.update(proofMessage(invitation));
    org.json.JSONArray certificate = new org.json.JSONArray();
    for (byte b : cert) certificate.put(b & 255);
    org.json.JSONArray signature = new org.json.JSONArray();
    for (byte b : signer.sign()) signature.put(b & 255);
    JSONObject join =
        new JSONObject()
            .put("version", 1)
            .put("token", invitation.getString("token"))
            .put("certificate", certificate)
            .put("signature", signature);
    JSONObject reply = exchange(invitation, join, true, 305000);
    if (!reply.getBoolean("accepted")) throw new IOException("Pairing declined or expired");
    JSONObject profile =
        new JSONObject()
            .put("host", invitation.getString("host"))
            .put("port", invitation.getInt("service_port"))
            .put("certificate", invitation.getString("certificate"));
    android.util.AtomicFile file =
        new android.util.AtomicFile(new File(context.getFilesDir(), "peer.json"));
    FileOutputStream out = file.startWrite();
    try {
      out.write(profile.toString().getBytes(StandardCharsets.UTF_8));
      file.finishWrite(out);
    } catch (Exception e) {
      file.failWrite(out);
      throw e;
    }
  }

  private JSONObject exchange(JSONObject peer, JSONObject request, boolean pairing, int timeout)
      throws Exception {
    byte[] pinned = Base64.decode(peer.getString("certificate"), Base64.DEFAULT);
    X509TrustManager verifier =
        new X509TrustManager() {
          public X509Certificate[] getAcceptedIssuers() {
            return new X509Certificate[0];
          }

          public void checkClientTrusted(X509Certificate[] chain, String auth)
              throws CertificateException {
            throw new CertificateException("client use only");
          }

          public void checkServerTrusted(X509Certificate[] chain, String auth)
              throws CertificateException {
            if (chain.length != 1 || !MessageDigest.isEqual(chain[0].getEncoded(), pinned))
              throw new CertificateException("Device identity mismatch");
            chain[0].checkValidity();
          }
        };
    X509KeyManager identity =
        new X509KeyManager() {
          public String chooseClientAlias(
              String[] types, Principal[] issuers, java.net.Socket socket) {
            return ALIAS;
          }

          public String[] getClientAliases(String type, Principal[] issuers) {
            return new String[] {ALIAS};
          }

          public String chooseServerAlias(
              String type, Principal[] issuers, java.net.Socket socket) {
            return null;
          }

          public String[] getServerAliases(String type, Principal[] issuers) {
            return null;
          }

          public X509Certificate[] getCertificateChain(String alias) {
            try {
              return new X509Certificate[] {(X509Certificate) keys.getCertificate(ALIAS)};
            } catch (Exception e) {
              throw new IllegalStateException("Device certificate unavailable", e);
            }
          }

          public PrivateKey getPrivateKey(String alias) {
            try {
              return (PrivateKey) keys.getKey(ALIAS, null);
            } catch (Exception e) {
              throw new IllegalStateException("Device key unavailable", e);
            }
          }
        };
    SSLContext tls = SSLContext.getInstance("TLS");
    tls.init(new KeyManager[] {identity}, new TrustManager[] {verifier}, new SecureRandom());
    try (java.net.Socket transport = new java.net.Socket()) {
      transport.connect(new InetSocketAddress(peer.getString("host"), peer.getInt("port")), 3000);
      transport.setSoTimeout(timeout);
      try (SSLSocket socket =
          (SSLSocket)
              tls.getSocketFactory()
                  .createSocket(transport, "dot.local", peer.getInt("port"), true)) {
        socket.setEnabledProtocols(new String[] {"TLSv1.3"});
        socket.setSoTimeout(timeout);
        socket.startHandshake();
        byte[] data =
            (pairing
                    ? NativeBridge.checkPair(request.toString(), false)
                    : NativeBridge.checkService(request.toString(), true))
                .getBytes(StandardCharsets.UTF_8);
        DataOutputStream out = new DataOutputStream(socket.getOutputStream());
        out.writeInt(data.length);
        out.write(data);
        out.flush();
        DataInputStream in = new DataInputStream(socket.getInputStream());
        int length = in.readInt();
        if (length < 0 || length > 262144) throw new IOException("Frame limit");
        byte[] response = new byte[length];
        in.readFully(response);
        JSONObject result =
            new JSONObject(
                pairing
                    ? new String(response, StandardCharsets.UTF_8)
                    : NativeBridge.checkService(
                        new String(response, StandardCharsets.UTF_8), false));
        if (!pairing && result.getString("service").equals("error"))
          throw new IOException(result.getString("message"));
        return result;
      }
    }
  }
}
