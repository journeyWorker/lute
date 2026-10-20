<!--
User-level agent context for Kata sandboxes, installed as ~/.omp/agent/AGENTS.md
by kata-create.sh.

Managed by KantoRegion/forca (omp/AGENTS.md). Edit it there and run
`forca sync`; local edits in a consumer repo are overwritten.
-->
# Kata sandbox notes

## Work artifacts go to the team NAS

Captures, recordings, renders, drafts, review material, handoff notes and any
other large or throwaway output do not belong in git or on this sandbox's disk
(it disappears with the sandbox). Put it on the team NAS with rclone. The
`nas` remote is preconfigured in the environment (`RCLONE_CONFIG_NAS_*`).

- One folder per unit of work: `nas:projects/<repo>/<YYYY-MM-DD-topic>/`
  (`<repo>` is the repository name, e.g. `bard`; `<topic>` is a short kebab-case
  subject; today's date).
- Upload: `rclone copy <local-dir> nas:projects/<repo>/<YYYY-MM-DD-topic>/`; for
  a single file: `rclone copyto <file> nas:projects/<repo>/<YYYY-MM-DD-topic>/<name>`.
- List, download, delete: `rclone lsf nas:projects/<repo>/<YYYY-MM-DD-topic>/`,
  `rclone copy nas:projects/<repo>/<YYYY-MM-DD-topic>/ <local-dir>`,
  `rclone deletefile nas:projects/<repo>/<YYYY-MM-DD-topic>/<name>`,
  `rclone purge nas:projects/<repo>/<YYYY-MM-DD-topic>/`.
- `rclone lsd nas:` shows one bucket, `projects`. Nothing else on the NAS is
  reachable from here and no other credentials are needed.
- The same folder on the operator's Mac is
  `/Volumes/MoeLive/storage/journey-temp/projects/<repo>/<YYYY-MM-DD-topic>/`,
  so tell the user that path when you hand artifacts over.
- A finished `rclone copy` means the NAS gateway accepted the upload; the file
  reaches the NAS within seconds (longer for big files), and `rclone lsl` on
  the folder lists it from the gateway before that. Tell the user the folder
  and that it may take a moment to show up on the Mac.
- Never print or copy the `RCLONE_CONFIG_NAS_*` values, and do not `rclone sync`
  or `purge` a folder you did not create.
