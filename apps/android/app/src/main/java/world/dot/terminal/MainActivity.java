package world.dot.terminal;

import android.app.Activity;
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
public final class MainActivity extends Activity {
  private final ScheduledExecutorService worker = Executors.newSingleThreadScheduledExecutor();
  private ScheduledFuture<?> polling;
  private TextView status;
  private EditText input;
  private TerminalView terminal;
  private String endpoint, token;
  // Only the single worker accesses controller state.
  private long generation = 0, sequence = 1;
  private volatile boolean foreground = false;
  private static final int BG = 0xff0c1118,
      INK = 0xffdbe5ed,
      MUTED = 0xff889ba9,
      GREEN = 0xff8be4c0;

  @Override
  public void onCreate(Bundle state) {
    super.onCreate(state);
    if (!BuildConfig.DEBUG)
      getWindow()
          .setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE);
    var prefs = getSharedPreferences("usb-development", MODE_PRIVATE);
    endpoint = prefs.getString("endpoint", "");
    token = prefs.getString("token", "");
    String supplied = getIntent().getStringExtra("pairing");
    if (BuildConfig.DEBUG && supplied != null && supplied.matches("[0-9a-f]{64}")) {
      token = supplied;
      endpoint = "http://127.0.0.1:17842/rpc";
      prefs.edit().putString("endpoint", endpoint).putString("token", token).apply();
    }
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setBackgroundColor(BG);
    root.setPadding(dp(18), dp(8), dp(18), dp(10));
    root.setOnApplyWindowInsetsListener(
        (view, insets) -> {
          android.graphics.Insets bars =
              insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.ime());
          view.setPadding(dp(18), bars.top + dp(8), dp(18), bars.bottom + dp(10));
          return insets;
        });
    TextView brand = label("●  DOT TERMINAL", 21, GREEN);
    brand.setPadding(0, dp(8), 0, dp(12));
    root.addView(brand);
    TextView subtitle = label("ONE SESSION. ANY SCREEN.", 11, MUTED);
    root.addView(subtitle);
    status = label("USB development connection · ready to pair", 13, MUTED);
    status.setPadding(0, dp(16), 0, dp(10));
    root.addView(status);
    LinearLayout actions = new LinearLayout(this);
    addButton(actions, "Take control", () -> connect());
    addButton(actions, "Disconnect", () -> disconnect());
    root.addView(actions);
    terminal = new TerminalView();
    LinearLayout.LayoutParams tp = new LinearLayout.LayoutParams(-1, 0, 1);
    tp.topMargin = dp(12);
    tp.bottomMargin = dp(12);
    root.addView(terminal, tp);
    LinearLayout keys = new LinearLayout(this);
    addButton(keys, "Esc", () -> send("\u001b"));
    addButton(keys, "Tab", () -> send("\t"));
    addButton(keys, "Ctrl C", () -> send("\u0003"));
    addButton(keys, "↑", () -> send("\u001b[A"));
    addButton(keys, "↓", () -> send("\u001b[B"));
    root.addView(keys);
    LinearLayout entry = new LinearLayout(this);
    input = new EditText(this);
    input.setSingleLine(true);
    input.setTextColor(INK);
    input.setHintTextColor(MUTED);
    input.setHint("Command on Mac…");
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
    root.addView(entry);
    TextView footer = label("Rust core  /  Mac session owner  /  USB only", 10, MUTED);
    footer.setPadding(0, dp(10), 0, 0);
    root.addView(footer);
    setContentView(root);
    if (token.isEmpty()) show("Pair this development app from your Mac first.");
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
    b.setText(title);
    b.setTextSize(11);
    b.setAllCaps(false);
    b.setTextColor(INK);
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

  private void connect() {
    worker.execute(
        () -> {
          try {
            generation = 0;
            JSONObject lease = rpc(op("acquire").put("takeover", true));
            generation = lease.getLong("generation");
            sequence = 1;
            rpc(op("resize").put("generation", generation).put("cols", 48).put("rows", 24));
            show("Connected · Mac shell · you have control");
            refresh();
          } catch (Exception e) {
            generation = 0;
            show(e.getMessage());
          }
        });
  }

  private void disconnect() {
    worker.execute(
        () -> {
          long old = generation;
          generation = 0;
          try {
            if (old != 0) rpc(op("release").put("generation", old));
            show("Disconnected · session continues on Mac");
          } catch (Exception e) {
            show("Disconnected · release unconfirmed");
          }
        });
  }

  private void submit() {
    String s = input.getText().toString();
    if (s.isEmpty()) return;
    send(s + "\r");
  }

  private void send(String text) {
    worker.execute(
        () -> {
          if (generation == 0) {
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
            refresh();
          } catch (Exception e) {
            generation = 0;
            show("Input unconfirmed; not retried. Take control to inspect.");
          }
        });
  }

  private void refresh() throws Exception {
    JSONObject screen = rpc(op("screen"));
    JSONArray a = screen.getJSONArray("lines");
    String[] lines = new String[a.length()];
    for (int i = 0; i < a.length(); i++) lines[i] = a.getString(i);
    int cols = screen.getInt("cols"),
        row = screen.getInt("cursor_row"),
        col = screen.getInt("cursor_col");
    runOnUiThread(() -> terminal.update(lines, cols, row, col));
    if (screen.getBoolean("exited")) {
      generation = 0;
      show("Shell exited · final screen retained");
    }
  }

  @Override
  protected void onStart() {
    super.onStart();
    foreground = true;
    polling =
        worker.scheduleWithFixedDelay(
            () -> {
              if (foreground && generation != 0)
                try {
                  refresh();
                } catch (Exception e) {
                  generation = 0;
                  show("Connection lost · session stays on Mac");
                }
            },
            0,
            250,
            TimeUnit.MILLISECONDS);
  }

  @Override
  protected void onStop() {
    foreground = false;
    if (polling != null) polling.cancel(false);
    super.onStop();
  }

  @Override
  protected void onDestroy() {
    worker.shutdownNow();
    super.onDestroy();
  }

  final class TerminalView extends View {
    private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
    private String[] lines = {
      "Welcome to DOT Terminal.",
      "",
      "Connect over USB to take control",
      "of a persistent Mac shell."
    };
    private int columns = 48, row = -1, column = 0;

    TerminalView() {
      super(MainActivity.this);
      setContentDescription("Terminal screen");
      p.setTypeface(Typeface.MONOSPACE);
    }

    void update(String[] s, int c, int r, int x) {
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
      c.drawColor(0xff121c27);
      float pad = dp(10);
      p.setTextSize((getWidth() - 2 * pad) / Math.max(columns, 1) / 0.602f);
      float line = Math.min(p.getTextSize() * 1.5f, (getHeight() - 2 * pad) / 24f);
      p.setTextSize(Math.min(p.getTextSize(), line / 1.3f));
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
    }
  }
}
