package world.dot.terminal;

import android.graphics.*;
import android.os.Bundle;
import android.view.*;
import android.view.inputmethod.EditorInfo;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.*;
import org.json.*;

/** Thin Android view. Rust validates the wire contract; the Mac owns PTY and VT state. */
public final class MainActivity extends androidx.activity.ComponentActivity {
  private android.content.ClipData previousClipboard;
  private String receivedClipboard;
  private boolean clipboardUndoAvailable;
  private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor();
  private ScheduledFuture<?> polling;
  private TextView status;
  private final SessionSignals signals = new SessionSignals();
  private TextView syncStatus;
  private final Runnable showSignals =
      new Runnable() {
        public void run() {
          syncStatus.setText(signals.summary(android.os.SystemClock.elapsedRealtime()));
          if (foreground) rootView.postDelayed(this, 1000);
        }
      };
  private EditText input;
  private TerminalView terminal;
  private String endpoint, token;
  private DeviceLink deviceLink;
  // Only the single worker accesses controller state.
  private volatile long generation = 0;
  private long sequence = 1;
  private volatile boolean viewing;
  private volatile int viewportCols = 48, viewportRows = 24;
  private String sessionLabel = "Paired session";
  private TextView sessionTitle;
  private boolean ctrlArmed;
  private Button ctrlKey;
  private final StringBuilder pendingInput = new StringBuilder();
  private long pendingGeneration;
  private final Runnable flushInput =
      () -> {
        String text = pendingInput.toString();
        pendingInput.setLength(0);
        if (generation == pendingGeneration && generation != 0) sendNow(text);
      };
  private volatile boolean handoffOffered = false;
  private final androidx.activity.result.ActivityResultLauncher<
          com.journeyapps.barcodescanner.ScanOptions>
      scanner =
          registerForActivityResult(
              new com.journeyapps.barcodescanner.ScanContract(),
              result -> {
                if (result.getContents() != null) receiveCode(result.getContents());
              });
  private volatile boolean foreground = false;
  private LinearLayout rootView;
  private float terminalSp = 14;
  private String terminalFont = "monospace";
  private int SURFACE = 0xff22312b;
  private int BG = 0xff0c1118, INK = 0xffdbe5ed, MUTED = 0xff889ba9, GREEN = 0xff8be4c0;

  @Override
  public void onCreate(Bundle state) {
    super.onCreate(state);
    if (!BuildConfig.DEBUG)
      getWindow()
          .setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE);
    try {
      deviceLink = new DeviceLink(this);
    } catch (Exception e) {
      throw new IllegalStateException("Device identity unavailable", e);
    }
    var prefs = getSharedPreferences("usb-development", MODE_PRIVATE);
    endpoint = prefs.getString("endpoint", "");
    token = prefs.getString("token", "");
    String supplied = getIntent().getStringExtra("pairing");
    if (BuildConfig.DEBUG && supplied != null && supplied.matches("[0-9a-f]{64}")) {
      token = supplied;
      endpoint = "http://127.0.0.1:17842/rpc";
      prefs.edit().putString("endpoint", endpoint).putString("token", token).apply();
    }
    var appearancePrefs = getSharedPreferences("appearance", MODE_PRIVATE);
    terminalSp = Math.max(8, Math.min(32, appearancePrefs.getFloat("size", 14)));
    terminalFont = appearancePrefs.getString("font", "monospace");
    setPalette(appearancePrefs.getString("theme", "Forest"));
    LinearLayout root = new LinearLayout(this);
    rootView = root;
    root.setOrientation(LinearLayout.VERTICAL);
    root.setBackgroundColor(BG);
    root.setPadding(dp(8), dp(4), dp(8), dp(4));
    root.setOnApplyWindowInsetsListener(
        (view, insets) -> {
          android.graphics.Insets bars =
              insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.ime());
          view.setPadding(dp(8), bars.top + dp(4), dp(8), bars.bottom + dp(4));
          return insets;
        });
    LinearLayout heading = new LinearLayout(this);
    sessionTitle = label("DOT · Paired session", 15, GREEN);
    sessionTitle.setTag("mobile.session.title");
    sessionTitle.setGravity(Gravity.CENTER_VERTICAL);
    heading.addView(sessionTitle, new LinearLayout.LayoutParams(0, dp(48), 2));
    addButton(heading, "Session", () -> sessionDialog());
    root.addView(heading);
    status = label("Disconnected · tap View to connect", 12, MUTED);
    status.setTag("mobile.connection.status");
    status.setPadding(dp(4), 0, 0, dp(4));
    root.addView(status);
    LinearLayout actions = new LinearLayout(this);
    addButton(actions, "View", () -> viewSession());
    addButton(actions, "Control", () -> connect());
    addButton(actions, "Disconnect", () -> disconnect());
    root.addView(actions);
    LinearLayout utilities = new LinearLayout(this);
    utilities.setOrientation(LinearLayout.VERTICAL);
    utilities.setVisibility(View.GONE);
    LinearLayout preferences = new LinearLayout(this);
    addButton(
        preferences,
        "Tools",
        () ->
            utilities.setVisibility(
                utilities.getVisibility() == View.GONE ? View.VISIBLE : View.GONE));
    addButton(preferences, "Appearance", () -> appearanceDialog());
    root.addView(preferences);
    root.addView(utilities);
    LinearLayout devices = new LinearLayout(this);
    addButton(
        devices,
        "Scan QR",
        () ->
            scanner.launch(
                new com.journeyapps.barcodescanner.ScanOptions()
                    .setDesiredBarcodeFormats("QR_CODE")
                    .setPrompt("Scan a DOT pairing or handoff code")
                    .setBeepEnabled(false)
                    .setOrientationLocked(false)));
    addButton(
        devices,
        "Pair link",
        () -> {
          EditText link = new EditText(this);
          link.setSingleLine(true);
          link.setInputType(
              android.text.InputType.TYPE_CLASS_TEXT
                  | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                  | android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
          link.setImeOptions(
              EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING);
          link.setHint("DOT invitation");
          new android.app.AlertDialog.Builder(this)
              .setTitle("Open DOT code")
              .setView(link)
              .setNegativeButton("Cancel", null)
              .setPositiveButton("Review", (d, w) -> receiveCode(link.getText().toString().trim()))
              .show();
        });
    addButton(devices, "Handoff", () -> offerHandoff());
    utilities.addView(devices);
    LinearLayout clipboard = new LinearLayout(this);
    addButton(clipboard, "Send clipboard", () -> sendClipboard());
    addButton(clipboard, "Get clipboard", () -> getClipboard());
    addButton(clipboard, "Undo", () -> undoClipboard());
    utilities.addView(clipboard);
    terminal = new TerminalView();
    LinearLayout.LayoutParams tp = new LinearLayout.LayoutParams(-1, 0, 1);
    tp.topMargin = dp(4);
    tp.bottomMargin = dp(4);
    FrameLayout stage = new FrameLayout(this);
    stage.addView(terminal, new FrameLayout.LayoutParams(-1, -1));
    root.addView(stage, tp);
    LinearLayout keyMenu = new LinearLayout(this);
    keyMenu.setOrientation(LinearLayout.VERTICAL);
    keyMenu.setBackgroundColor(SURFACE);
    keyMenu.setPadding(dp(4), dp(4), dp(4), dp(4));
    keyMenu.setVisibility(View.GONE);
    FrameLayout.LayoutParams menuPosition =
        new FrameLayout.LayoutParams(-1, dp(104), Gravity.BOTTOM);
    menuPosition.bottomMargin = dp(56);
    stage.addView(keyMenu, menuPosition);
    String[] titles = {"Esc", "Tab", "Ctrl", "↑", "↓", "←", "→", "⌫", "↵", "⌨"};
    String[] labels = {
      "Escape",
      "Tab",
      "Control modifier",
      "Arrow up",
      "Arrow down",
      "Arrow left",
      "Arrow right",
      "Backspace",
      "Enter",
      "Show keyboard"
    };
    String[] values = {
      "\u001b", "\t", "", "\u001b[A", "\u001b[B", "\u001b[D", "\u001b[C", "\u007f", "\r", ""
    };
    for (int row = 0; row < 2; row++) {
      LinearLayout keys = new LinearLayout(this);
      keyMenu.addView(keys);
      for (int column = 0; column < 5; column++) {
        final int key = row * 5 + column;
        addButton(
            keys,
            titles[key],
            () -> {
              if (key == 2) {
                ctrlArmed = !ctrlArmed;
                ctrlKey.setText(ctrlArmed ? "Ctrl ●" : "Ctrl");
                ctrlKey.setSelected(ctrlArmed);
              } else if (key == 9) terminal.openKeyboard();
              else send(values[key]);
            });
        View button = keys.getChildAt(column);
        button.setContentDescription(labels[key]);
        if (key == 2) ctrlKey = (Button) button;
      }
    }
    Button dot = new Button(this);
    dot.setText("●");
    dot.setTextColor(GREEN);
    dot.setContentDescription("Expand terminal keys");
    dot.setTag("mobile.input.key-menu");
    dot.setBackgroundTintList(android.content.res.ColorStateList.valueOf(SURFACE));
    dot.setPadding(0, 0, 0, 0);
    dot.setOnClickListener(
        v -> {
          boolean expanded = keyMenu.getVisibility() == View.GONE;
          keyMenu.setVisibility(expanded ? View.VISIBLE : View.GONE);
          dot.setContentDescription(expanded ? "Collapse terminal keys" : "Expand terminal keys");
        });
    stage.addView(dot, new FrameLayout.LayoutParams(dp(52), dp(52), Gravity.BOTTOM | Gravity.END));
    syncStatus = label("Sync · unknown", 11, MUTED);
    syncStatus.setTag("mobile.session.signals");
    syncStatus.setContentDescription("Session connection measurements");
    syncStatus.setMinHeight(dp(44));
    syncStatus.setGravity(Gravity.CENTER_VERTICAL);
    syncStatus.setOnClickListener(
        v ->
            new android.app.AlertDialog.Builder(this)
                .setTitle("Session signals")
                .setMessage(
                    signals.details(android.os.SystemClock.elapsedRealtime())
                        + "\n\n"
                        + "This view only. Peer sync, host output offset and physical pixel latency"
                        + " are unknown. No terminal text is collected.")
                .setPositiveButton("Close", null)
                .show());
    root.addView(syncStatus);
    LinearLayout entry = new LinearLayout(this);
    input = new EditText(this);
    input.setSingleLine(true);
    input.setTextColor(INK);
    input.setHintTextColor(MUTED);
    input.setHint("Review command, then send…");
    input.setTextSize(15);
    input.setInputType(
        android.text.InputType.TYPE_CLASS_TEXT
            | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            | android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
    input.setImeOptions(
        EditorInfo.IME_ACTION_SEND
            | EditorInfo.IME_FLAG_NO_EXTRACT_UI
            | EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING);
    input.setOnEditorActionListener(
        (v, id, event) -> {
          if (id == EditorInfo.IME_ACTION_SEND) {
            submit();
            return true;
          }
          return false;
        });
    entry.addView(input, new LinearLayout.LayoutParams(0, dp(52), 1));
    addButton(entry, "↵", () -> submit());
    entry.getChildAt(1).setLayoutParams(new LinearLayout.LayoutParams(dp(60), dp(48)));
    entry.setVisibility(View.GONE);
    addButton(
        preferences,
        "Compose",
        () -> entry.setVisibility(entry.getVisibility() == View.GONE ? View.VISIBLE : View.GONE));
    root.addView(entry);
    setContentView(root);
    updateSystemBars();
    if (!deviceLink.paired() && token.isEmpty())
      show("Scan an invitation from your other device to pair.");
  }

  private void setPalette(String theme) {
    if (theme.equals("Paper")) {
      SURFACE = 0xffe1e7dc;
      BG = 0xfffaf8f2;
      INK = 0xff202d29;
      MUTED = 0xff52665b;
      GREEN = 0xff176544;
    } else if (theme.equals("Midnight")) {
      SURFACE = 0xff293452;
      BG = 0xff101424;
      INK = 0xffe0e7ff;
      MUTED = 0xffa5b3d6;
      GREEN = 0xff9dbaff;
    } else if (theme.equals("High contrast")) {
      SURFACE = 0xff262626;
      BG = 0xff000000;
      INK = 0xffffffff;
      MUTED = 0xffcccccc;
      GREEN = 0xffffff70;
    } else {
      SURFACE = 0xff22312b;
      BG = 0xff111519;
      INK = 0xffd4dedc;
      MUTED = 0xffa1b3a9;
      GREEN = 0xffadf4cf;
    }
  }

  private void updateSystemBars() {
    var controller = getWindow().getInsetsController();
    int flags =
        WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
            | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
    if (controller != null)
      controller.setSystemBarsAppearance(
          BG == 0xfffaf8f2 ? WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS : 0, flags);
  }

  private void recolor(View view) {
    if (view instanceof Button button)
      button.setBackgroundTintList(android.content.res.ColorStateList.valueOf(SURFACE));
    if (view instanceof TextView text) text.setTextColor(INK);
    if (view instanceof EditText edit) edit.setHintTextColor(MUTED);
    if (view instanceof android.view.ViewGroup group)
      for (int i = 0; i < group.getChildCount(); i++) recolor(group.getChildAt(i));
  }

  private void appearanceDialog() {
    LinearLayout form = new LinearLayout(this);
    form.setOrientation(LinearLayout.VERTICAL);
    form.setPadding(dp(20), dp(12), dp(20), dp(12));
    form.addView(label("Terminal size (sp, 8–32)", 14, INK));
    EditText size = new EditText(this);
    size.setInputType(
        android.text.InputType.TYPE_CLASS_NUMBER | android.text.InputType.TYPE_NUMBER_FLAG_DECIMAL);
    size.setText(Float.toString(terminalSp));
    form.addView(size);
    form.addView(label("Local font family (use a monospace font)", 14, INK));
    EditText font = new EditText(this);
    font.setSingleLine(true);
    font.setText(terminalFont);
    form.addView(font);
    Spinner theme = new Spinner(this);
    String[] names = {"Forest", "Midnight", "Paper", "High contrast"};
    theme.setAdapter(
        new ArrayAdapter<String>(this, android.R.layout.simple_spinner_dropdown_item, names) {
          @Override
          public View getView(int position, View recycled, android.view.ViewGroup parent) {
            View v = super.getView(position, recycled, parent);
            ((TextView) v).setTextColor(INK);
            v.setBackgroundColor(BG);
            return v;
          }

          @Override
          public View getDropDownView(int position, View recycled, android.view.ViewGroup parent) {
            View v = super.getDropDownView(position, recycled, parent);
            ((TextView) v).setTextColor(INK);
            v.setBackgroundColor(BG);
            return v;
          }
        });
    String selected = getSharedPreferences("appearance", MODE_PRIVATE).getString("theme", "Forest");
    for (int i = 0; i < names.length; i++) if (names[i].equals(selected)) theme.setSelection(i);
    form.addView(theme);
    form.addView(
        label(
            "Exact text size stays readable. Drag the terminal to see columns or rows outside the"
                + " view. The DOT key menu follows this palette. The main keyboard uses your"
                + " Gboard/IME theme settings.",
            13,
            MUTED));
    android.app.AlertDialog dialog =
        new android.app.AlertDialog.Builder(this)
            .setTitle("Appearance")
            .setView(form)
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Apply", null)
            .create();
    form.setBackgroundColor(BG);
    recolor(form);
    dialog.setOnShowListener(
        d ->
            dialog
                .getButton(android.app.AlertDialog.BUTTON_POSITIVE)
                .setOnClickListener(
                    v -> {
                      try {
                        float value = Float.parseFloat(size.getText().toString());
                        String family = font.getText().toString().trim();
                        if (!Float.isFinite(value) || value < 8 || value > 32) {
                          size.setError("Choose 8 to 32 sp");
                          return;
                        }
                        if (!family.matches("[A-Za-z0-9 _-]{1,80}")) {
                          font.setError("Enter a local font family");
                          return;
                        }
                        terminalSp = value;
                        terminalFont = family;
                        String name = names[theme.getSelectedItemPosition()];
                        setPalette(name);
                        getSharedPreferences("appearance", MODE_PRIVATE)
                            .edit()
                            .putFloat("size", value)
                            .putString("font", family)
                            .putString("theme", name)
                            .apply();
                        rootView.setBackgroundColor(BG);
                        recolor(rootView);
                        updateSystemBars();
                        terminal.invalidate();
                        terminal.scheduleResize();
                        dialog.dismiss();
                      } catch (NumberFormatException e) {
                        size.setError("Enter a valid size");
                      }
                    }));
    dialog.show();
  }

  private void receiveCode(String capsule) {
    try {
      if (capsule.startsWith("dot-handoff:v1:")) {
        receiveHandoff(capsule);
        return;
      }
      JSONObject invitation = deviceLink.invitation(capsule);
      String code = deviceLink.confirmationCode(invitation);
      String grants =
          "Terminal: "
              + invitation.getBoolean("terminal")
              + "\nRead clipboard: "
              + invitation.getBoolean("clipboard_read")
              + "\nWrite clipboard: "
              + invitation.getBoolean("clipboard_write");
      new android.app.AlertDialog.Builder(this)
          .setTitle("Pair this device?")
          .setMessage(
              "Confirm the same code on your other device:\n\n"
                  + code
                  + "\n\n"
                  + grants
                  + "\n\n"
                  + "Only continue with an invitation you requested. This replaces the current"
                  + " connection profile after approval.")
          .setNegativeButton("Cancel", null)
          .setPositiveButton(
              "Request pairing",
              (d, w) ->
                  worker.execute(
                      () -> {
                        try {
                          long old = generation;
                          generation = 0;
                          if (old != 0) {
                            try {
                              rpc(op("release").put("generation", old));
                            } catch (Exception ignored) {
                            }
                          }
                          show("Confirm on your other device: " + code);
                          deviceLink.pair(invitation);
                          show("Paired · tap Take control");
                        } catch (Exception e) {
                          show("Pairing failed or expired; previous profile kept");
                        }
                      }))
          .show();
    } catch (Exception e) {
      show("Invalid DOT code");
    }
  }

  private void offerHandoff() {
    worker.execute(
        () -> {
          try {
            if (generation == 0 || !deviceLink.paired()) {
              show("Take control before handing off");
              return;
            }
            JSONObject identity = deviceLink.call(new JSONObject().put("service", "identity"));
            JSONObject state = rpc(op("status"));
            JSONObject offer = rpc(op("offer_handoff").put("generation", generation));
            handoffOffered = true;
            JSONObject body =
                new JSONObject()
                    .put("node", identity.getString("node_id"))
                    .put("session", state.getString("session"))
                    .put("ticket", offer.getString("ticket"));
            String capsule =
                "dot-handoff:v1:"
                    + android.util.Base64.encodeToString(
                        body.toString().getBytes(StandardCharsets.UTF_8),
                        android.util.Base64.URL_SAFE
                            | android.util.Base64.NO_PADDING
                            | android.util.Base64.NO_WRAP);
            com.google.zxing.common.BitMatrix matrix =
                new com.google.zxing.MultiFormatWriter()
                    .encode(capsule, com.google.zxing.BarcodeFormat.QR_CODE, 720, 720);
            Bitmap image = Bitmap.createBitmap(720, 720, Bitmap.Config.ARGB_8888);
            for (int y = 0; y < 720; y++)
              for (int x = 0; x < 720; x++)
                image.setPixel(x, y, matrix.get(x, y) ? Color.BLACK : Color.WHITE);
            runOnUiThread(
                () -> {
                  if (!foreground) {
                    cancelHandoff();
                    return;
                  }
                  ImageView view = new ImageView(this);
                  view.setImageBitmap(image);
                  view.setAdjustViewBounds(true);
                  view.setMaxHeight(
                      Math.max(dp(120), getResources().getDisplayMetrics().heightPixels - dp(230)));
                  view.setScaleType(ImageView.ScaleType.FIT_CENTER);
                  new android.app.AlertDialog.Builder(this)
                      .setTitle("Handoff · expires in 2 minutes")
                      .setView(view)
                      .setMessage(
                          "Scan with a device already paired to this host. Your session keeps"
                              + " running. Keep this code private.")
                      .setPositiveButton("Close", (d, w) -> cancelHandoff())
                      .setOnCancelListener(d -> cancelHandoff())
                      .show();
                });
          } catch (Exception e) {
            show("Could not offer handoff");
          }
        });
  }

  private void cancelHandoff() {
    worker.execute(
        () -> {
          if (handoffOffered && generation != 0)
            try {
              rpc(op("cancel_handoff").put("generation", generation));
            } catch (Exception ignored) {
            }
          handoffOffered = false;
        });
  }

  private void receiveHandoff(String capsule) throws Exception {
    if (capsule.length() > 2048 || !deviceLink.paired())
      throw new IOException("Pair with this host first");
    JSONObject offer =
        new JSONObject(
            new String(
                android.util.Base64.decode(capsule.substring(15), android.util.Base64.URL_SAFE),
                StandardCharsets.UTF_8));
    if (offer.length() != 3
        || !offer.getString("ticket").matches("[0-9a-f]{64}")
        || !offer.getString("session").matches("[0-9a-f]{32}"))
      throw new IOException("Invalid handoff");
    new android.app.AlertDialog.Builder(this)
        .setTitle("Continue this session here?")
        .setMessage(
            "Accepting transfers control from the previous device. Pairing and clipboard"
                + " permissions stay unchanged.")
        .setNegativeButton("Cancel", null)
        .setPositiveButton(
            "Accept handoff",
            (d, w) ->
                worker.execute(
                    () -> {
                      try {
                        JSONObject identity =
                            deviceLink.call(new JSONObject().put("service", "identity"));
                        if (!identity.getString("node_id").equals(offer.getString("node"))
                            || !rpc(op("status"))
                                .getString("session")
                                .equals(offer.getString("session")))
                          throw new IOException("Different host or session");
                        generation = 0;
                        JSONObject lease =
                            rpc(op("accept_handoff").put("ticket", offer.getString("ticket")));
                        generation = lease.getLong("generation");
                        sequence = 1;
                        handoffOffered = false;
                        viewing = true;
                        resizeToViewport();
                        refresh();
                        show("Handoff accepted · you have control");
                      } catch (Exception e) {
                        show("Handoff unavailable, expired, or for another host");
                      }
                    }))
        .show();
  }

  private int dp(int n) {
    return Math.round(n * getResources().getDisplayMetrics().density);
  }

  private TextView label(String text, int size, int color) {
    TextView t = new TextView(this);
    t.setText(text);
    t.setTextSize(size);
    t.setTextColor(color);
    return t;
  }

  private void addButton(LinearLayout row, String title, Runnable action) {
    Button b = new Button(this);
    b.setContentDescription(title);
    b.setTooltipText(title);
    b.setText(
        switch (title) {
          case "Appearance" -> "◐";
          case "Tools" -> "⋯";
          case "Compose" -> "✎";
          case "Session" -> "☰";
          default -> title;
        });
    b.setTag(
        switch (title) {
          case "View" -> "mobile.action.view";
          case "Control" -> "mobile.action.control";
          case "Disconnect" -> "mobile.action.disconnect";
          case "Tools" -> "mobile.tools";
          case "Ctrl" -> "mobile.input.modifier";
          default -> "mobile.action." + title;
        });
    b.setTextSize(11);
    b.setAllCaps(false);
    b.setTextColor(INK);
    b.setBackgroundTintList(android.content.res.ColorStateList.valueOf(SURFACE));
    b.setMinWidth(0);
    b.setMinimumWidth(0);
    row.addView(b, new LinearLayout.LayoutParams(0, dp(48), 1));
    b.setOnClickListener(v -> action.run());
  }

  private void show(String s) {
    runOnUiThread(() -> status.setText(s));
  }

  private JSONObject op(String kind) throws JSONException {
    return new JSONObject().put("type", kind);
  }

  private JSONObject rpc(JSONObject operation) throws Exception {
    long start = android.os.SystemClock.elapsedRealtime();
    String kind = operation.optString("type");
    try {
      JSONObject result = rpcRaw(operation);
      signals.record(kind, android.os.SystemClock.elapsedRealtime() - start, true);
      return result;
    } catch (Exception e) {
      signals.record(kind, android.os.SystemClock.elapsedRealtime() - start, false);
      throw e;
    }
  }

  private JSONObject rpcRaw(JSONObject operation) throws Exception {
    if (deviceLink.paired()) {
      JSONObject request = new JSONObject().put("version", 1).put("operation", operation);
      JSONObject response =
          deviceLink
              .call(new JSONObject().put("service", "terminal").put("request", request))
              .getJSONObject("response");
      if (response.getString("type").equals("error"))
        throw new IOException(response.getString("message"));
      return response;
    }
    if (!BuildConfig.DEBUG
        || !endpoint.equals("http://127.0.0.1:17842/rpc")
        || !token.matches("[0-9a-f]{64}")) throw new IOException("USB pairing required");
    String wire =
        NativeBridge.check(
            new JSONObject().put("version", 1).put("operation", operation).toString(), true);
    HttpURLConnection c = (HttpURLConnection) new URL(endpoint).openConnection();
    c.setConnectTimeout(2000);
    c.setReadTimeout(2500);
    c.setRequestMethod("POST");
    c.setDoOutput(true);
    c.setInstanceFollowRedirects(false);
    c.setRequestProperty("Authorization", "Bearer " + token);
    c.setRequestProperty("Content-Type", "application/json");
    // The development HTTP/1.0 bridge closes every response; never pool its sockets.
    c.setRequestProperty("Connection", "close");
    byte[] bytes = wire.getBytes(StandardCharsets.UTF_8);
    c.setFixedLengthStreamingMode(bytes.length);
    try {
      try (OutputStream out = c.getOutputStream()) {
        out.write(bytes);
      }
      if (c.getResponseCode() != 200)
        throw new IOException("Bridge refused request (" + c.getResponseCode() + ")");
      ByteArrayOutputStream out = new ByteArrayOutputStream();
      try (InputStream in = c.getInputStream()) {
        byte[] b = new byte[4096];
        int n;
        while ((n = in.read(b)) != -1) {
          if (out.size() + n > 262144) throw new IOException("Response too large");
          out.write(b, 0, n);
        }
      }
      JSONObject response =
          new JSONObject(NativeBridge.check(out.toString(StandardCharsets.UTF_8.name()), false));
      if (response.getString("type").equals("error"))
        throw new IOException(response.getString("message"));
      return response;
    } finally {
      c.disconnect();
    }
  }

  private void sendClipboard() {
    if (!deviceLink.paired()) {
      show("Pair an authenticated device first");
      return;
    }
    android.content.ClipboardManager clipboard =
        getSystemService(android.content.ClipboardManager.class);
    android.content.ClipData clip = clipboard.getPrimaryClip();
    if (clip == null || clip.getItemCount() != 1 || clip.getItemAt(0).getText() == null) {
      show("Copy one text item first");
      return;
    }
    if (clip.getDescription().getExtras() != null
        && clip.getDescription().getExtras().getBoolean("android.content.extra.IS_SENSITIVE")) {
      show("Sensitive clipboard item was not shared");
      return;
    }
    String text = clip.getItemAt(0).getText().toString();
    worker.execute(
        () -> {
          try {
            deviceLink.call(new JSONObject().put("service", "clipboard_set").put("text", text));
            show("Clipboard sent to paired Mac");
          } catch (Exception e) {
            show(e.getMessage());
          }
        });
  }

  private void getClipboard() {
    if (!deviceLink.paired()) {
      show("Pair an authenticated device first");
      return;
    }
    worker.execute(
        () -> {
          try {
            String text =
                deviceLink.call(new JSONObject().put("service", "clipboard_get")).getString("text");
            runOnUiThread(
                () -> {
                  if (!foreground) {
                    show("Return to DOT to receive clipboard");
                    return;
                  }
                  android.content.ClipboardManager manager =
                      getSystemService(android.content.ClipboardManager.class);
                  previousClipboard = manager.getPrimaryClip();
                  receivedClipboard = text;
                  clipboardUndoAvailable = true;
                  manager.setPrimaryClip(
                      android.content.ClipData.newPlainText("From paired DOT device", text));
                  show("Mac clipboard ready to paste");
                });
          } catch (Exception e) {
            show(e.getMessage());
          }
        });
  }

  private void undoClipboard() {
    android.content.ClipboardManager manager =
        getSystemService(android.content.ClipboardManager.class);
    android.content.ClipData current = manager.getPrimaryClip();
    if (!clipboardUndoAvailable) {
      show("No clipboard change to undo");
      return;
    }
    if (current == null
        || current.getItemCount() != 1
        || !receivedClipboard.contentEquals(
            current.getItemAt(0).getText() == null ? "" : current.getItemAt(0).getText())
        || !"From paired DOT device"
            .contentEquals(
                current.getDescription().getLabel() == null
                    ? ""
                    : current.getDescription().getLabel())) {
      show("Clipboard changed since receiving; left untouched");
    } else {
      if (previousClipboard == null) manager.clearPrimaryClip();
      else manager.setPrimaryClip(previousClipboard);
      show("Previous clipboard restored");
    }
    previousClipboard = null;
    receivedClipboard = null;
    clipboardUndoAvailable = false;
  }

  private void sessionDialog() {
    new android.app.AlertDialog.Builder(this)
        .setTitle(sessionLabel)
        .setMessage(
            "This paired gateway currently exposes one session. Viewing never takes control."
                + " Disconnecting leaves the shell running on its host.")
        .setPositiveButton("View", (d, w) -> viewSession())
        .setNeutralButton("Release control", (d, w) -> releaseControl(true))
        .setNegativeButton("Close", null)
        .show();
  }

  private void viewSession() {
    worker.execute(
        () -> {
          try {
            JSONObject state = rpc(op("status"));
            String id = state.getString("session");
            sessionLabel = "Session " + id.substring(0, Math.min(8, id.length()));
            runOnUiThread(() -> sessionTitle.setText("DOT · " + sessionLabel));
            viewing = true;
            refresh();
            if (viewing) show(generation == 0 ? "Live · view only" : "Live · you have control");
          } catch (Exception e) {
            viewing = false;
            show("Unavailable · tap View to retry");
          }
        });
  }

  private void connect() {
    acquire(false);
  }

  private void acquire(boolean takeover) {
    worker.execute(
        () -> {
          if (generation != 0) {
            show("You already have control");
            return;
          }
          try {
            JSONObject lease = rpc(op("acquire").put("takeover", takeover));
            generation = lease.getLong("generation");
            sequence = 1;
            viewing = true;
            resizeToViewport();
            refresh();
            if (generation != 0) show("Live · you have control");
          } catch (Exception e) {
            generation = 0;
            show("Control unavailable · view remains separate");
            if (!takeover)
              runOnUiThread(
                  () ->
                      new android.app.AlertDialog.Builder(this)
                          .setTitle("Request takeover?")
                          .setMessage(
                              "If another device controls this session, taking over will stop its"
                                  + " input. A connection failure may also prevent control.")
                          .setNegativeButton("Cancel", null)
                          .setPositiveButton("Take over", (d, w) -> acquire(true))
                          .show());
          }
        });
  }

  private void resizeToViewport() throws Exception {
    if (generation != 0)
      rpc(
          op("resize")
              .put("generation", generation)
              .put("cols", viewportCols)
              .put("rows", viewportRows));
  }

  private void disconnect() {
    releaseControl(false);
  }

  private void releaseControl(boolean keepViewing) {
    worker.execute(
        () -> {
          long old = generation;
          generation = 0;
          handoffOffered = false;
          viewing = keepViewing;
          try {
            if (old != 0) rpc(op("release").put("generation", old));
            show(keepViewing ? "Live · view only" : "Disconnected · shell stays on host");
          } catch (Exception e) {
            show("Input disabled · release unconfirmed");
          }
        });
  }

  private void submit() {
    String s = input.getText().toString();
    if (s.isEmpty()) return;
    send(s + "\r");
  }

  private void send(String text) {
    if (generation == 0) {
      show("Take control before sending input");
      return;
    }
    if (pendingGeneration != generation) {
      pendingInput.setLength(0);
      rootView.removeCallbacks(flushInput);
    }
    pendingGeneration = generation;
    if ((pendingInput.toString() + text).getBytes(StandardCharsets.UTF_8).length > 16384) {
      show("Input is too large");
      return;
    }
    boolean empty = pendingInput.length() == 0;
    pendingInput.append(text);
    if (empty) rootView.postDelayed(flushInput, 12);
  }

  private void sendNow(String text) {
    if (text.isEmpty()) return;
    final long expectedGeneration = generation;
    if (text.getBytes(StandardCharsets.UTF_8).length > 16384) {
      show("Input is too large");
      return;
    }
    worker.execute(
        () -> {
          if (generation == 0 || generation != expectedGeneration) {
            show("Take control before sending input");
            return;
          }
          try {
            JSONArray bytes = new JSONArray();
            for (byte b : text.getBytes(StandardCharsets.UTF_8)) bytes.put(b & 255);
            rpc(
                op("input")
                    .put("generation", generation)
                    .put("sequence", sequence)
                    .put("data", bytes));
            sequence++;
            runOnUiThread(
                () -> {
                  if (text.equals(input.getText().toString() + "\r")) input.setText("");
                });
          } catch (Exception e) {
            generation = 0;
            show("Input unconfirmed; not retried. Take control to inspect.");
          }
        });
  }

  private void refresh() throws Exception {
    if (generation != 0) {
      try {
        rpc(op("check_control").put("generation", generation));
      } catch (Exception e) {
        generation = 0;
        handoffOffered = false;
        show("Control transferred or connection lost · session stays on host");
        return;
      }
    }
    JSONObject screen = rpc(op("screen"));
    final long receivedAt = android.os.SystemClock.elapsedRealtime();
    signals.received(receivedAt);
    JSONArray a = screen.getJSONArray("lines");
    String[] lines = new String[a.length()];
    for (int i = 0; i < a.length(); i++) lines[i] = a.getString(i);
    int cols = screen.getInt("cols"),
        row = screen.getInt("cursor_row"),
        col = screen.getInt("cursor_col");
    runOnUiThread(
        () -> {
          terminal.update(lines, cols, row, col);
          signals.applied(receivedAt, android.os.SystemClock.elapsedRealtime());
        });
    if (screen.getBoolean("exited")) {
      generation = 0;
      viewing = false;
      show("Shell exited · final screen retained");
    }
  }

  @Override
  protected void onStart() {
    super.onStart();
    foreground = true;
    rootView.post(showSignals);
    polling =
        worker.scheduleWithFixedDelay(
            () -> {
              if (foreground && viewing)
                try {
                  refresh();
                } catch (Exception e) {
                  generation = 0;
                  viewing = false;
                  show("Connection lost · tap View to reconnect");
                }
            },
            0,
            250,
            TimeUnit.MILLISECONDS);
  }

  @Override
  protected void onStop() {
    foreground = false;
    rootView.removeCallbacks(showSignals);
    if (polling != null) polling.cancel(false);
    super.onStop();
  }

  @Override
  protected void onDestroy() {
    rootView.removeCallbacks(flushInput);
    pendingInput.setLength(0);
    terminal.removeCallbacks(terminal.resizeTask);
    worker.execute(
        () -> {
          try {
            if (generation != 0) rpc(op("release").put("generation", generation));
          } catch (Exception ignored) {
          }
          generation = 0;
        });
    worker.shutdown();
    super.onDestroy();
  }

  final class TerminalView extends View {
    private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
    private String[] lines = {
      "Welcome to DOT Terminal.",
      "",
      "Pair over Wi-Fi to view a shell.",
      "Drag to pan. Appearance sets text size."
    };
    private int columns = 48, row = -1, column = 0;
    private float panX, panY, touchX, touchY;
    private boolean dragged;

    @Override
    public boolean performClick() {
      super.performClick();
      openKeyboard();
      return true;
    }

    @Override
    public boolean onTouchEvent(MotionEvent e) {
      if (e.getAction() == MotionEvent.ACTION_DOWN) {
        touchX = e.getX();
        touchY = e.getY();
        dragged = false;
        return true;
      }
      if (e.getAction() == MotionEvent.ACTION_MOVE) {
        if (Math.abs(touchX - e.getX()) + Math.abs(touchY - e.getY()) > dp(3)) dragged = true;
        panX += touchX - e.getX();
        panY += touchY - e.getY();
        touchX = e.getX();
        touchY = e.getY();
        invalidate();
        return true;
      }
      if (e.getAction() == MotionEvent.ACTION_UP) {
        if (!dragged) performClick();
        return true;
      }
      return true;
    }

    TerminalView() {
      super(MainActivity.this);
      setTag("mobile.terminal.content");
      setContentDescription("Terminal screen. Tap to type, drag to pan.");
      setFocusable(true);
      setFocusableInTouchMode(true);
      p.setTypeface(Typeface.create(terminalFont, Typeface.NORMAL));
    }

    private final Runnable resizeTask =
        () ->
            worker.execute(
                () -> {
                  try {
                    resizeToViewport();
                  } catch (Exception e) {
                    generation = 0;
                    show("Resize unconfirmed · view only");
                  }
                });

    void scheduleResize() {
      p.setTypeface(Typeface.create(terminalFont, Typeface.NORMAL));
      p.setTextSize(terminalSp * getResources().getDisplayMetrics().scaledDensity);
      viewportCols = TerminalKeys.cells(getWidth() - dp(20), p.measureText("M"), 240);
      viewportRows = TerminalKeys.cells(getHeight() - dp(20), p.getTextSize() * 1.4f, 100);
      removeCallbacks(resizeTask);
      postDelayed(resizeTask, 150);
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
      scheduleResize();
    }

    void openKeyboard() {
      requestFocus();
      getSystemService(android.view.inputmethod.InputMethodManager.class).showSoftInput(this, 0);
    }

    @Override
    public boolean onCheckIsTextEditor() {
      return true;
    }

    private void commitInput(String text) {
      if (text.length() > 1
          && (text.contains("\n") || text.contains("\r") || text.contains("\u001b"))) {
        final String paste = text;
        new android.app.AlertDialog.Builder(MainActivity.this)
            .setTitle("Review terminal paste")
            .setMessage(text)
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Send to shell", (d, w) -> send(paste))
            .show();
        return;
      }
      if (ctrlArmed && text.codePointCount(0, text.length()) == 1) {
        text = TerminalKeys.control(text);
        ctrlArmed = false;
        ctrlKey.setText("Ctrl");
        // Modifier state is shown by accessibility/status as well as the key label.

      }
      send(text);
    }

    @Override
    public android.view.inputmethod.InputConnection onCreateInputConnection(EditorInfo info) {
      info.inputType =
          android.text.InputType.TYPE_CLASS_TEXT
              | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
              | android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD;
      info.imeOptions =
          EditorInfo.IME_ACTION_NONE
              | EditorInfo.IME_FLAG_NO_EXTRACT_UI
              | EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING;
      return new android.view.inputmethod.BaseInputConnection(this, true) {
        @Override
        public boolean commitText(CharSequence text, int position) {
          commitInput(text.toString());
          getEditable().clear();
          return true;
        }

        @Override
        public boolean finishComposingText() {
          String text = getEditable().toString();
          super.finishComposingText();
          getEditable().clear();
          if (!text.isEmpty()) commitInput(text);
          return true;
        }

        @Override
        public boolean deleteSurroundingText(int before, int after) {
          if (getEditable().length() > 0) return super.deleteSurroundingText(before, after);
          send(
              "\u007f".repeat(Math.max(0, Math.min(before, 100)))
                  + "\u001b[3~".repeat(Math.max(0, Math.min(after, 100))));
          return true;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int before, int after) {
          if (getEditable().length() > 0)
            return super.deleteSurroundingTextInCodePoints(before, after);
          return deleteSurroundingText(before, after);
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
          return terminal.dispatchKeyEvent(event);
        }

        @Override
        public boolean performEditorAction(int action) {
          send("\r");
          return true;
        }
      };
    }

    @Override
    public boolean onKeyDown(int code, KeyEvent event) {
      String text = TerminalKeys.special(code);
      if (text != null) {
        send(text);
        return true;
      }
      if (text == null) {
        int ch = event.getUnicodeChar(event.getMetaState() & ~KeyEvent.META_CTRL_MASK);
        if (ch == 0 || (ch & KeyCharacterMap.COMBINING_ACCENT) != 0)
          return super.onKeyDown(code, event);
        text = new String(Character.toChars(ch));
        if (event.isCtrlPressed()) text = TerminalKeys.control(text);
        if (event.isAltPressed()) text = "\u001b" + text;
      }
      commitInput(text);
      return true;
    }

    void update(String[] s, int c, int r, int x) {
      if (java.util.Arrays.equals(lines, s) && columns == c && row == r && column == x) return;
      lines = s;
      columns = c;
      row = r;
      column = x;
      setContentDescription("Terminal screen: " + String.join("\n", s));
      invalidate();
    }

    @Override
    protected void onDraw(Canvas c) {
      super.onDraw(c);
      c.drawColor(BG);
      float pad = dp(10);
      p.setTypeface(Typeface.create(terminalFont, Typeface.NORMAL));
      p.setTextSize(terminalSp * getResources().getDisplayMetrics().scaledDensity);
      float line = p.getTextSize() * 1.4f;
      panX =
          Math.max(
              0, Math.min(panX, Math.max(0, columns * p.measureText("M") + 2 * pad - getWidth())));
      panY = Math.max(0, Math.min(panY, Math.max(0, lines.length * line + 2 * pad - getHeight())));
      c.save();
      c.translate(-panX, -panY);
      p.setColor(INK);
      for (int i = 0; i < lines.length; i++) c.drawText(lines[i], pad, pad + (i + 1) * line, p);
      if (row >= 0) {
        p.setColor(GREEN);
        float cw = p.measureText("M");
        c.drawRect(
            pad + column * cw,
            pad + (row + 1) * line + 2,
            pad + (column + 1) * cw,
            pad + (row + 1) * line + 4,
            p);
      }
      c.restore();
    }
  }
}
