# Efficient GIF review / 高效剪辑

Inspect duration and available timeline first. Query actions, then use `framelet_analyze` for unknown/multiple changes. This combines spatial differences with recorded input; it does not recognize semantic importance and cannot reconstruct unrecorded history.

Probe several requested timestamps together. Reuse each returned half-open frame interval to avoid decoding or visually inspecting the same frame again. Use numeric reference/region comparisons for intermediate boundary checks; export a sheet only when images will answer a visual question.

Binary search is useful only inside a known monotonic boundary with a fixed reference and stable comparison region. Split A→B→A, reverse scrolling and multiple actions into intervals before local refinement. Animations can disturb the predicate; review ambiguous boundary images rather than trusting a scalar threshold alone.

Keep head/tail context around the chosen action, then edit to a new path. Inspect the derived duration and batch-probe the opening, action and closing positions. Derived asset/frame identity changes; original event/sample identity and computer time remain the references for traceable edits.
