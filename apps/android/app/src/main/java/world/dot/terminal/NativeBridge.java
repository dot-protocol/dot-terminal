package world.dot.terminal;

final class NativeBridge {
  static {
    System.loadLibrary("dot_terminal_android");
  }

  private NativeBridge() {}

  static native String check(String json, boolean request);
}
