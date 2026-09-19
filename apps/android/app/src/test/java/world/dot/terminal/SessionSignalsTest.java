package world.dot.terminal;

import static org.junit.Assert.*;

import org.junit.Test;

public class SessionSignalsTest {
  @Test
  public void boundedSamplesAndFreshness() {
    SessionSignals s = new SessionSignals();
    assertTrue(s.summary(0).contains("unknown"));
    for (int i = 0; i < 200; i++) s.record("screen", i, true);
    assertEquals(193, s.percentile("screen", .95));
    s.received(1);
    assertTrue(s.summary(1).contains("applying"));
    s.applied(1, 3);
    assertTrue(s.summary(3).contains("snapshot received"));
    assertTrue(s.summary(3002).contains("stale"));
    s.record("private-name", 4, false);
    assertFalse(s.details(4).contains("private-name"));
  }
}
