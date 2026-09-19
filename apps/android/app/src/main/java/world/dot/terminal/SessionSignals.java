package world.dot.terminal;

import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;

/** Bounded metadata only; never receives terminal text, input, identities or credentials. */
final class SessionSignals {
  private final Map<String, long[]> samples = new LinkedHashMap<>();
  private final Map<String, Integer> counts = new LinkedHashMap<>();
  private long received = -1, applied = -1, failures;

  SessionSignals() {
    for (String stage : new String[] {"screen", "input", "resize", "apply"}) {
      samples.put(stage, new long[120]);
      counts.put(stage, 0);
    }
  }

  synchronized void record(String stage, long millis, boolean ok) {
    if (!ok) failures++;
    if (!samples.containsKey(stage) || millis < 0) return;
    int n = counts.get(stage);
    samples.get(stage)[n % 120] = millis;
    counts.put(stage, n == Integer.MAX_VALUE ? 120 : n + 1);
  }

  synchronized void received(long now) {
    received = now;
  }

  synchronized void applied(long receipt, long now) {
    applied = receipt;
    record("apply", Math.max(0, now - receipt), true);
  }

  synchronized long percentile(String stage, double quantile) {
    int n = Math.min(120, counts.get(stage));
    if (n == 0) return -1;
    long[] sorted = Arrays.copyOf(samples.get(stage), n);
    Arrays.sort(sorted);
    return sorted[(int) Math.ceil(n * quantile) - 1];
  }

  synchronized String summary(long now) {
    String state =
        received < 0
            ? "unknown"
            : now - received > 3000
                ? "stale"
                : applied < received ? "applying" : "snapshot received";
    return "Sync · " + state + " · " + display(percentile("screen", .95)) + " p95";
  }

  private String display(long value) {
    return value < 0 ? "—" : value + " ms";
  }

  synchronized String details(long now) {
    StringBuilder s = new StringBuilder(summary(now));
    for (String stage : samples.keySet())
      s.append("\n")
          .append(stage)
          .append(" p50 / p95: ")
          .append(display(percentile(stage, .5)))
          .append(" / ")
          .append(display(percentile(stage, .95)))
          .append(" · ")
          .append(Math.min(120, counts.get(stage)))
          .append(" samples");
    return s
        + "\nFailed calls: "
        + failures
        + "\nResponse age: "
        + (received < 0 ? "unknown" : display(Math.max(0, now - received)));
  }
}
