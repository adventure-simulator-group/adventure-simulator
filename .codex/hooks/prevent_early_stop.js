// Ask Codex for one more turn when a stop looks like a progress report.
// The `stop_hook_active` escape hatch prevents an infinite continuation loop.

let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => (input += chunk));
process.stdin.on("end", () => {
  const event = JSON.parse(input);
  if (event.stop_hook_active) {
    process.stdout.write(JSON.stringify({ continue: true }));
    return;
  }

  const message = String(event.last_assistant_message || "").toLowerCase();
  const markers = [
    "next steps",
    "remaining work",
    "still need to",
    "i'll continue",
    "i will continue",
    "progress update",
    "partially complete",
    "implementation remains",
    "the next task is",
  ];

  if (markers.some((marker) => message.includes(marker))) {
    process.stdout.write(
      JSON.stringify({
        decision: "block",
        reason:
          "The last response appears to be an intermediate progress report. " +
          "Continue the original request now. Do not send another progress-only " +
          "response. If the work changes a user-visible surface, initialize or restart " +
          "the relevant local server and demonstrate the result when the environment " +
          "permits it. Stop only after completing and verifying the requested outcome, " +
          "or when a real blocker requires user input.",
      }),
    );
  } else {
    process.stdout.write(JSON.stringify({ continue: true }));
  }
});
