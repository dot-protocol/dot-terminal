package world.dot.terminal;
/** IME owns a pending word; only commit/finish emits it, exactly once. */
final class TerminalComposition {
  private String pending="";
  String text(){return pending;}
  void update(String text){pending=text.length()<=16384?text:"";}
  void clear(){pending="";}
  String commit(String text){clear();return text;}
  String finish(){String text=pending;clear();return text;}
  void backspace(){if(!pending.isEmpty())pending=pending.substring(0,pending.offsetByCodePoints(pending.length(),-1));}
}
