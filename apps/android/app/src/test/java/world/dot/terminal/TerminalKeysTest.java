package world.dot.terminal;

import static org.junit.Assert.*;

import org.junit.Test;

public class TerminalKeysTest {
  @Test
  public void controlChordsDoNotCorruptUnicode() {
    assertEquals("\3", TerminalKeys.control("c"));
    assertEquals("\0", TerminalKeys.control(" "));
    assertEquals("\u001b", TerminalKeys.control("["));
    assertEquals("\u007f", TerminalKeys.control("?"));
    assertEquals("é", TerminalKeys.control("é"));
    assertEquals("🙂", TerminalKeys.control("🙂"));
  }

  @Test
  public void terminalKeysDifferFromAndroidNavigation() {
    assertEquals("\r", TerminalKeys.special(66));
    assertEquals("\u007f", TerminalKeys.special(67));
    assertEquals("\u001b[D", TerminalKeys.special(21));
    assertNull(TerminalKeys.special(4)); // Android Back hides keyboard; not terminal Escape.
  }

  @Test
  public void viewportRespectsInsetsAndBounds() {
    assertEquals(80, TerminalKeys.cells(800, 10, 500));
    assertEquals(24, TerminalKeys.cells(480, 20, 200));
    assertEquals(12, TerminalKeys.cells(240, 20, 200));
    assertEquals(2, TerminalKeys.cells(-10, 20, 200));
    assertEquals(2, TerminalKeys.cells(800, Float.NaN, 200));
    assertEquals(500, TerminalKeys.cells(10000, 1, 500));
  }
}
