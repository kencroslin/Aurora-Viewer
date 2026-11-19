# Aurora Custom builds

Published downloads appear on the [Releases page](https://github.com/kencroslin/Aurora-Viewer/releases). Each release includes an Apple Silicon Mac app and a Windows x64 portable ZIP. Extract the entire Windows ZIP and then open `aurora-viewer.exe`. On macOS, extract the ZIP and open the app directly or move it to Applications.

## Build from source

```sh
git clone --branch custom-builds https://github.com/kencroslin/Aurora-Viewer.git
```

The `automation` branch is the GitHub landing page and contains the workflow. Viewer development uses `custom-builds`.

## Merge and build

The [Custom Mac and Windows builds workflow](https://github.com/kencroslin/Aurora-Viewer/actions/workflows/custom-builds.yml) checks official Aurora `main` daily at 10:27 UTC. To start a build yourself, open the workflow and choose **Run workflow** on `automation`. Leave **Build even when the official source has not changed** checked to rebuild the current version.

A conflict stops the run and lists the affected files and commit IDs in the Actions summary. Resolve the conflict on a branch based on `custom-builds`, review the merge, and push the reviewed result to `custom-builds` before running the workflow again.

When the merge succeeds, GitHub builds Apple Silicon macOS and Windows x64 from the same source archive. Both jobs must pass before the workflow updates `custom-builds` and creates a release draft with both ZIPs, checksums, and the matching source archive. A manual run can rebuild the current branch even when upstream has no new commits.

Before publishing the first public draft, confirm both builds launch on real machines and complete the public viewer identity and download-page disclosures. Aurora Custom is the working name. See the upstream [trademark policy](https://github.com/Aurora-Viewer/Aurora-Viewer/blob/main/TRADEMARKS.md) and the [Second Life third-party viewer policy](https://secondlife.com/corporate/third-party-viewers). The initial Mac packages use ad hoc signing and Windows packages are unsigned.
