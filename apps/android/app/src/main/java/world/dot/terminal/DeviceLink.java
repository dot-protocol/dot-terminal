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
      transport.setSoTimeout(4000);
      try (SSLSocket socket =
          (SSLSocket)
              tls.getSocketFactory()
                  .createSocket(transport, "dot.local", peer.getInt("port"), true)) {
        socket.setEnabledProtocols(new String[] {"TLSv1.3"});
        socket.setSoTimeout(4000);
        socket.startHandshake();
        byte[] data =
            NativeBridge.checkService(request.toString(), true).getBytes(StandardCharsets.UTF_8);
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
                NativeBridge.checkService(new String(response, StandardCharsets.UTF_8), false));
        if (result.getString("service").equals("error"))
          throw new IOException(result.getString("message"));
        return result;
      }
    }
  }
}
