# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.

-keep class com.unigal.ayaka_gui.* {
  native <methods>;
}

-keepclassmembers class com.unigal.ayaka_gui.TauriActivity {
  getAppClass(...);
  getVersion();
}

-keep class com.unigal.ayaka_gui.RustWebView {
  public <init>(...);
  loadUrlMainThread(...);
}

-keep class com.unigal.ayaka_gui.Ipc {
  public <init>(...);
  @android.webkit.JavascriptInterface public <methods>;
}

-keep class com.unigal.ayaka_gui.RustWebChromeClient,com.unigal.ayaka_gui.RustWebViewClient {
  public <init>(...);
}

-keep class com.unigal.ayaka_gui.MainActivity {
  public getPluginManager();
}

-keep class androidx.appcompat.app.AppCompatActivity { }
