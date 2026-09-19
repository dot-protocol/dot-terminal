import Cocoa
import WebKit

final class AppDelegate: NSObject, NSApplicationDelegate, WKNavigationDelegate, WKUIDelegate {
    var window: NSWindow!
    var web: WKWebView!
    var backend: Process?
    var origin: String?
    var reader = Data()
    func applicationDidFinishLaunching(_ notification: Notification) {
        let menu = NSMenu()
        let item = NSMenuItem(); menu.addItem(item)
        let appMenu = NSMenu(); appMenu.addItem(withTitle: "Quit DOT Terminal", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"); item.submenu=appMenu
        let editItem=NSMenuItem();menu.addItem(editItem);let edit=NSMenu(title:"Edit");editItem.submenu=edit
        for (title,selector,key) in [("Copy","copy:","c"),("Paste","paste:","v"),("Select All","selectAll:","a")] {edit.addItem(withTitle:title,action:Selector(selector),keyEquivalent:key)}
        NSApp.mainMenu=menu
        let config=WKWebViewConfiguration();config.websiteDataStore = .nonPersistent()
        web=WKWebView(frame:NSRect(x:0,y:0,width:1220,height:800),configuration:config);web.autoresizingMask=[.width,.height];web.navigationDelegate=self;web.uiDelegate=self
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:1220,height:800),styleMask:[.titled,.closable,.miniaturizable,.resizable],backing:.buffered,defer:false)
        window.title="DOT Terminal";window.minSize=NSSize(width:680,height:440);window.contentView=web;window.center();window.makeKeyAndOrderFront(nil)
        window.backgroundColor=NSColor(calibratedRed:0.067,green:0.082,blue:0.098,alpha:1)
        NSApp.activate(ignoringOtherApps:true)
        guard let resources=Bundle.main.resourceURL else {return}
        let process=Process();backend=process
        process.executableURL=Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent("dot-terminal-desktop")
        process.arguments=["--resource-binary",Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent("dot-terminal-resources").path,"--assets",resources.appendingPathComponent("web").path,"--session-binary",Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent("dot-terminal").path]
        let python=FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/DOT Terminal/integrations/iterm/bin/python3")
        if FileManager.default.fileExists(atPath:python.path){process.arguments! += ["--iterm-python",python.path,"--iterm-bridge",resources.appendingPathComponent("iterm_bridge.py").path]}
        let pipe=Pipe();process.standardOutput=pipe;process.standardError=FileHandle.nullDevice
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data=handle.availableData
            guard let self=self, !data.isEmpty else {handle.readabilityHandler=nil;return}
            self.reader.append(data)
            if let text=String(data:self.reader,encoding:.utf8),let line=text.split(separator:"\n").first,let url=URL(string:String(line)),url.host=="127.0.0.1" {
                handle.readabilityHandler=nil
                DispatchQueue.main.async {self.origin="http://127.0.0.1:\(url.port!)";self.web.load(URLRequest(url:url))}
            }
        }
        process.terminationHandler={ [weak self] _ in DispatchQueue.main.async {self?.window.title="DOT Terminal — service stopped; reopen the app"} }
        do {try process.run()} catch {window.title="DOT Terminal — could not start local service"}
    }
    func webView(_ webView:WKWebView,decidePolicyFor action:WKNavigationAction,decisionHandler:@escaping(WKNavigationActionPolicy)->Void){
        guard let url=action.request.url,let origin=origin,url.scheme=="http",url.host=="127.0.0.1",origin=="http://127.0.0.1:\(url.port ?? 0)" else {decisionHandler(.cancel);return}
        decisionHandler(.allow)
    }
    func webView(_ webView:WKWebView,createWebViewWith configuration:WKWebViewConfiguration,for action:WKNavigationAction,windowFeatures:WKWindowFeatures)->WKWebView?{
        if let url=action.request.url,url.host=="127.0.0.1",origin=="http://127.0.0.1:\(url.port ?? 0)" {NSWorkspace.shared.open(url)}
        return nil
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender:NSApplication)->Bool {true}
    func applicationWillTerminate(_ notification:Notification){backend?.terminate()}
}
let app=NSApplication.shared
let delegate=AppDelegate();app.delegate=delegate;app.setActivationPolicy(.regular);app.run()
