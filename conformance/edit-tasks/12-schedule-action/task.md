Add a schedule-consuming action to the meteor record flow with an explicit clock effect, while preserving a path that completes before the festival deadline and all existing schedule constraints.

The expected `beat:perseids.tally` scheduling change is the direct consequence of adding authored `advances="slot"`; no engine-owned clock state is edited. The trap removes the schedule-consuming construct while changing the existing spend period, so it must be flagged as a scheduling diff.

Korean task name: 일정에 소모 행동 추가
