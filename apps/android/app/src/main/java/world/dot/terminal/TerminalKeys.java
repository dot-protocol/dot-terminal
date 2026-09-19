package world.dot.terminal;

/** Pure input/geometry rules; Android key constants are stable wire values. */
final class TerminalKeys {
  static int cells(float available, float cell, int max) {
    if (!Float.isFinite(available) || !Float.isFinite(cell) || cell <= 0) return 2;
    return Math.max(2, Math.min(max, (int) Math.floor(available / cell)));
  }

  static String control(String text) {
    if (text.length() != 1) return text;
    char c = Character.toUpperCase(text.charAt(0));
    if (c == ' ' || c == '@') return "\0";
    if (c >= 'A' && c <= '_') return Character.toString((char) (c & 31));
    if (c == '?') return "\u007f";
    return text;
  }

  static String special(int code) {
    return switch (code) {
      case 66, 160 -> "\r";
      case 67 -> "\u007f";
      case 112 -> "\u001b[3~";
      case 61 -> "\t";
      case 111 -> "\u001b";
      case 19 -> "\u001b[A";
      case 20 -> "\u001b[B";
      case 21 -> "\u001b[D";
      case 22 -> "\u001b[C";
      case 122 -> "\u001b[H";
      case 123 -> "\u001b[F";
      case 92 -> "\u001b[5~";
      case 93 -> "\u001b[6~";
      default -> null;
    };
  }
}
