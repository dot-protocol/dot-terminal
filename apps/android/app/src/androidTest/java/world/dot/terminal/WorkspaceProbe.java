package world.dot.terminal;

import android.app.Instrumentation;
import android.os.Bundle;
import org.json.JSONObject;

/** Opt-in network acceptance against a disposable operator-specified session. No UI or agent input. */
public final class WorkspaceProbe extends Instrumentation {
  private Bundle args;
  @Override public void onCreate(Bundle arguments){args=arguments;start();}
  @Override public void onStart(){
    Bundle result=new Bundle();
    try {
      DeviceLink link=new DeviceLink(getTargetContext());
      JSONObject devices=link.call(new JSONObject().put("service","workspace").put("path","devices"));
      if(devices.optInt("status")!=200)throw new Exception("Device catalog unavailable");
      JSONObject external=link.call(new JSONObject().put("service","workspace").put("path","external"));
      if(external.optInt("status")!=200)throw new Exception("Remote catalog unavailable");
      String path=args.getString("viewPath","");
      if(!path.matches("external/[a-z0-9-]+/s_[a-zA-Z0-9]+/view"))throw new Exception("Explicit disposable viewPath required");
      JSONObject opened=call(link,path,new JSONObject().put("op","open"));
      String view=opened.getString("view");
      try {
        long after=0;int frames=0;boolean replay=false;
        for(int i=0;i<30&&!replay;i++){
          JSONObject read=call(link,path,new JSONObject().put("op","read").put("view",view).put("after",after));
          org.json.JSONArray list=read.getJSONArray("frames");
          for(int j=0;j<list.length();j++){
            JSONObject f=list.getJSONObject(j);long seq=f.getLong("seq");
            if(seq!=after+1)throw new Exception("Stream sequence gap");after=seq;frames++;
            String control=f.getJSONObject("frame").optString("control","");
            if(control.contains("replay_complete"))replay=true;
          }
          if(!replay)Thread.sleep(100);
        }
        if(!replay)throw new Exception("No complete replay barrier");
        String command="printf 'DOT_PHONE_%s\n' READY\r".replace("\\r","\r");
        StringBuilder hex=new StringBuilder();for(byte b:command.getBytes(java.nio.charset.StandardCharsets.UTF_8))hex.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
        call(link,path,new JSONObject().put("op","send").put("view",view).put("data",hex.toString()));
        call(link,path,new JSONObject().put("op","send").put("view",view).put("control",new JSONObject().put("type","resize").put("cols",110).put("rows",32)));
        boolean echoed=false;StringBuilder output=new StringBuilder();
        for(int i=0;i<30&&!echoed;i++){
          JSONObject read=call(link,path,new JSONObject().put("op","read").put("view",view).put("after",after));
          org.json.JSONArray list=read.getJSONArray("frames");
          for(int j=0;j<list.length();j++){
            JSONObject f=list.getJSONObject(j);if(f.getLong("seq")!=after+1)throw new Exception("Input response sequence gap");after=f.getLong("seq");
            String data=f.getJSONObject("frame").optString("data","");byte[] bytes=new byte[data.length()/2];for(int k=0;k<bytes.length;k++)bytes[k]=(byte)Integer.parseInt(data.substring(k*2,k*2+2),16);
            output.append(new String(bytes,java.nio.charset.StandardCharsets.UTF_8));
          }
          echoed=output.indexOf("DOT_PHONE_READY")>=0;if(!echoed)Thread.sleep(100);
        }
        if(!echoed)throw new Exception("No command output returned from disposable shell");
        result.putString("result","PASS: Android paired mTLS catalog, ordered replay, real VPS command round trip and resize request; view closed");
      } finally {call(link,path,new JSONObject().put("op","close").put("view",view));}
      finish(-1,result);
    } catch(Exception e){result.putString("result","FAIL: "+e.getMessage());finish(0,result);}
  }
  private JSONObject call(DeviceLink link,String path,JSONObject body)throws Exception{
    JSONObject r=link.call(new JSONObject().put("service","workspace").put("path",path).put("body",body));
    if(r.optInt("status")!=200)throw new Exception("Workspace request failed: "+r.optInt("status"));
    return r.getJSONObject("body");
  }
}
