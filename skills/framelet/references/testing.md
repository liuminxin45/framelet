# Software verification / 软件验收

Define the observable result before capture: a final UI state, an interaction sequence, or a brief visual defect. Use one screenshot for a single final state; 1 FPS can show stable states, while multi-step flows may need 3–5 FPS and short defects need a higher rate. Read current guidance and report actual sampling; low FPS cannot prove absence of a brief error.

Start and await readiness before the triggering action. Use the host's UI tools and existing user authorization. Stop and await a successful terminal recording result. Query the timeline by action/time, narrow by device or event kinds, and follow cursors when completeness matters. Default summaries omit mousemove traffic; request it explicitly for a pointer-path assertion.

Review frames surrounding the action, rather than equating the click timestamp with the application's response. Report event/sample/asset identities and explicit offsets where they support the test. Check capture warnings, dropped input and integrity metadata before claiming a complete trace. A recording is evidence of observed behavior, not a substitute for an application assertion.
