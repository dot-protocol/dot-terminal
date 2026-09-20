package world.dot.terminal;
import org.junit.Test;import static org.junit.Assert.*;
public class TerminalCompositionTest {
 @Test public void compositionCommitsOnce(){TerminalComposition c=new TerminalComposition();c.update("p");c.update("pw");c.update("pwd");assertEquals("pwd",c.commit("pwd"));assertEquals("",c.finish());assertEquals("",c.finish());}
 @Test public void compositionDeletionKeepsUnicodeIntact(){TerminalComposition c=new TerminalComposition();c.update("a😀");c.backspace();assertEquals("a",c.text());assertEquals("a",c.finish());assertEquals("",c.text());}
}
