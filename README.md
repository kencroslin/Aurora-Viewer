# Aurora Custom builds

The **Custom Mac and Windows builds** workflow runs daily at 10:27 UTC and on demand. It merges official Aurora `main` into `custom-builds`. A conflict stops the run and lists the affected files in the Actions summary. Resolve the merge in Git, review it, and push the reviewed result to `custom-builds` before running the workflow again.

When the merge succeeds, GitHub builds Apple Silicon macOS and Windows x64 from the same source archive. Both jobs must pass before the workflow updates `custom-builds` and creates a release draft with both ZIPs, checksums, and the matching source archive. A manual run can rebuild the current branch even when upstream has no new commits.

GitHub schedules and manual dispatch use the default branch. The workflow lives on `automation`, while `custom-builds` is the branch to clone for the viewer and `main` is the official source snapshot.

Before publishing the first public draft, confirm both builds launch on real machines and complete the public viewer identity and download-page disclosures. Aurora Custom is the working name. See the upstream [trademark policy](https://github.com/Aurora-Viewer/Aurora-Viewer/blob/main/TRADEMARKS.md) and the [Second Life third-party viewer policy](https://secondlife.com/corporate/third-party-viewers). The initial Mac packages use ad hoc signing and Windows packages are unsigned.
