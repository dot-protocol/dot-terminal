package world.dot.terminal;

import android.annotation.SuppressLint;
import android.os.Bundle;
import android.webkit.*;
import android.view.*;
import java.io.*;
import java.util.Map;
import java.util.concurrent.*;
import org.json.JSONObject;

/** Bundled, origin-locked shared UI. Remote data is JSON, never executable content. */
public final class WorkspaceActivity extends androidx.activity.ComponentActivity {
  private static final String ORIGIN = "https://workspace.dot.invalid";
  private WebView web;
  private DeviceLink link;
  private final ThreadPoolExecutor workers = new ThreadPoolExecutor(4, 4, 0L,
      TimeUnit.MILLISECONDS, new ArrayBlockingQueue<>(32));
  private volatile boolean closed;
  private volatile boolean terminalFocused;
  private volatile String terminalTarget="";

  @SuppressLint({"SetJavaScriptEnabled", "AddJavascriptInterface"})
  @Override public void onCreate(Bundle saved) {
    super.onCreate(saved);
    if(!BuildConfig.DEBUG)getWindow().setFlags(WindowManager.LayoutParams.FLAG_SECURE,WindowManager.LayoutParams.FLAG_SECURE);
    try { link = new DeviceLink(this); } catch (Exception e) { finish(); return; }
    web = new TerminalWebView();
    web.setBackgroundColor(0xff111519);
    android.widget.FrameLayout container = new android.widget.FrameLayout(this);
    container.setBackgroundColor(0xff111519);
    container.addView(web, new android.widget.FrameLayout.LayoutParams(-1,-1));
    container.setOnApplyWindowInsetsListener((view, insets) -> {
      android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.ime());
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom);
      return insets;
    });
    WebSettings settings = web.getSettings();
    settings.setJavaScriptEnabled(true);
    settings.setDomStorageEnabled(true);
    settings.setAllowFileAccess(false);
    settings.setAllowContentAccess(false);
    settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
    settings.setSupportMultipleWindows(false);
    web.setWebViewClient(new WebViewClient() {
      @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
        return true; // No remote page or subframe may inherit the native bridge.
      }
      @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
        String path = request.getUrl().getPath();
        if (!"https".equals(request.getUrl().getScheme()) || !"workspace.dot.invalid".equals(request.getUrl().getHost())
            || request.getUrl().getPort()!=-1 || path==null || path.contains("..") || !path.matches("/[a-zA-Z0-9_./-]*")) return denied();
        if (path.equals("/")) path="/index.html";
        String mime = path.endsWith(".js") ? "text/javascript" : path.endsWith(".css") ? "text/css" : path.endsWith(".json") ? "application/json" : "text/html";
        try {
          return new WebResourceResponse(mime, "UTF-8", 200, "OK", Map.of(
            "Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'",
            "Cache-Control", "no-store"), getAssets().open("workspace"+path));
        } catch (IOException e) { return denied(); }
      }
    });
    web.addJavascriptInterface(new Bridge(), "DotWorkspace");
    setContentView(container);
    container.requestApplyInsets();
    web.loadUrl(ORIGIN+"/");
  }
  private static WebResourceResponse denied() {
    return new WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", Map.of(), new ByteArrayInputStream(new byte[0]));
  }
  private void emitInput(String text, String target) {
    if(text.isEmpty() || closed)return;
    runOnUiThread(() -> { if(!closed)web.evaluateJavascript("window.dotNativeInput("+JSONObject.quote(text)+","+JSONObject.quote(target)+")",null); });
  }
  private void composition(String text) {
    runOnUiThread(() -> { if(!closed)web.evaluateJavascript("window.dotComposition("+JSONObject.quote(text)+")",null); });
  }
  private final class TerminalWebView extends WebView {
    TerminalWebView(){super(WorkspaceActivity.this);}
    @Override public android.view.inputmethod.InputConnection onCreateInputConnection(android.view.inputmethod.EditorInfo info) {
      android.view.inputmethod.InputConnection base=super.onCreateInputConnection(info);
      if(base==null)return null;
      info.imeOptions |= android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI;
      return new android.view.inputmethod.InputConnectionWrapper(base,false) {
        final TerminalComposition composing=new TerminalComposition();
        final String target=terminalFocused?terminalTarget:null;
        boolean current(){return target!=null&&target.equals(terminalTarget);}
        void send(String text){if(current())emitInput(text,target);}

        @Override public boolean setComposingText(CharSequence text,int cursor) {
          if(!terminalFocused)return target==null?super.setComposingText(text,cursor):true;
          if(!current())return true;
          composing.update(text.toString());composition(composing.text());return true;
        }
        @Override public boolean commitText(CharSequence text,int cursor) {
          if(!terminalFocused)return target==null?super.commitText(text,cursor):true;
          send(composing.commit(text.toString()));composition("");return true;
        }
        @Override public boolean finishComposingText() {
          if(!terminalFocused){composing.clear();return target==null?super.finishComposingText():true;}
          send(composing.finish());composition("");return true;
        }
        @Override public boolean deleteSurroundingText(int before,int after) {
          if(!terminalFocused)return target==null?super.deleteSurroundingText(before,after):true;
          if(!composing.text().isEmpty()){composing.backspace();composition(composing.text());}
          else send("\u007f".repeat(Math.max(0,Math.min(128,before))));
          return true;
        }
        @Override public boolean deleteSurroundingTextInCodePoints(int before,int after){if(!terminalFocused&&target==null)return super.deleteSurroundingTextInCodePoints(before,after);return deleteSurroundingText(before,after);}
        @Override public boolean sendKeyEvent(KeyEvent event) {
          if(!terminalFocused)return target==null?super.sendKeyEvent(event):true;
          int code=event.getUnicodeChar();
          if(event.getKeyCode()==KeyEvent.KEYCODE_ENTER){if(event.getAction()==KeyEvent.ACTION_DOWN){send(composing.finish());composition("");send("\r");}return true;}
          if(event.getKeyCode()==KeyEvent.KEYCODE_DEL){if(event.getAction()==KeyEvent.ACTION_DOWN)deleteSurroundingText(1,0);return true;}
          if(code>0&&(code&KeyCharacterMap.COMBINING_ACCENT)==0){if(event.getAction()==KeyEvent.ACTION_DOWN){send(composing.finish());composition("");send(new String(Character.toChars(code)));}return true;}
          return super.sendKeyEvent(event);
        }
      };
    }
  }
  private final class Bridge {
    @JavascriptInterface public void terminalFocus(boolean focused, String target) {
      boolean focusChanged=terminalFocused!=focused;
      terminalFocused=focused;
      String next=target==null?"":target;
      if(focusChanged||!next.equals(terminalTarget)){
        terminalTarget=next;
        runOnUiThread(()->{if(!closed)((android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).restartInput(web);});
      }
    }

    @JavascriptInterface public void request(String id, String encoded) {
      if (closed || id==null || !id.matches("[0-9]{1,16}") || encoded==null || encoded.length()>131072) return;
      try {
        workers.execute(() -> {
          JSONObject reply;
          try {
            JSONObject request = new JSONObject(encoded);
            reply = link.call(new JSONObject().put("service","workspace").put("path",request.getString("path")).put("body",request.opt("body")));
          } catch (Exception e) {
            reply=new JSONObject();try { reply.put("error",e.getMessage()==null?"Connection unavailable":e.getMessage()); } catch(Exception ignored) { }
          }
          final String result=reply.toString();
          runOnUiThread(() -> { if(!closed)web.evaluateJavascript("window.dotWorkspaceReply("+JSONObject.quote(id)+","+result+")",null); });
        });
      } catch (RejectedExecutionException e) {
        runOnUiThread(() -> { if(!closed)web.evaluateJavascript("window.dotWorkspaceReply("+JSONObject.quote(id)+",{error:'Connection busy; input was not retried'})",null); });
      }
    }
  }
  @Override protected void onDestroy() {
    closed=true;workers.shutdownNow();
    if(web!=null){web.removeJavascriptInterface("DotWorkspace");web.destroy();}
    super.onDestroy();
  }
}
