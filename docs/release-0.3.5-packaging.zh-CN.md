# Viden 0.3.5 打包与产物矩阵

English version: [release-0.3.5-packaging.md](release-0.3.5-packaging.md)

日期：2026-09-13。由 [release-0.3.5-plan.zh-CN.md](release-0.3.5-plan.zh-CN.md)
的批次 P1 撰写，该批次负责"macOS（arm64、x86_64）、Windows、Linux 的可复现产物"
与退出标准 7。

本文件记录实际构建了什么、没有构建什么以及原因。它是证据，不是发布声明。此处
没有任何内容被打标签、用真实签名身份签名、公证、上传、发布或推送。

## 来源

| 字段 | 值 |
| --- | --- |
| 分支 | `claude/release-packaging-p1` |
| 工作树 | `.worktrees/release-packaging-p1` |
| 基线 | `claude/int-0.3.5` 的 `f2298f4f` |
| 工作区 / CLI 版本 | `0.1.30`（`Cargo.toml` 的 `[workspace.package]`） |
| TUI 版本 | `0.3.5` |
| GUI 版本 | `0.1.0-rc.5`（R1 升到 `rc.6` 并重新构建） |
| Core 检查点 | `0.3.7`，代码检查点 `39ed155d` |

以上版本就是产物携带的版本。R1 会抬升它们；因此这里记录的产物是打包路径的
候选形态证据，而不是发布单元的最终资产。

## 授权边界

本地构建与离线检查无需授权，已执行。每一个对外可见的步骤都需要逐步显式"go"；
本批次没有获得任何授权，因此下表中每一项都记为**未执行 —— 缺少授权**，而不是
被静默跳过。

| 对外步骤 | 状态 |
| --- | --- |
| 分发 `.github/workflows/release.yml` | 未执行 —— 缺少授权 |
| 创建 Git 标签 | 未执行 —— 缺少授权 |
| 创建或编辑 GitHub Release | 未执行 —— 缺少授权 |
| 上传发布资产 | 未执行 —— 缺少授权 |
| 使用 Developer ID 身份进行代码签名 | 未执行 —— 缺少授权，且宿主机没有签名身份（`security find-identity -v -p codesigning` → `0 valid identities found`） |
| 公证（`notarytool`） | 未执行 —— 缺少授权，且环境中没有 Apple ID / Team ID 凭据 |
| Homebrew tap（`wikieden/homebrew-tap`） | 未执行 —— 缺少授权 |
| 任何分支的 `git push` | 未执行 —— 本批次不合并、不推送 |

## 工具链记录

| 组件 | 值 |
| --- | --- |
| 宿主机 | macOS 26.6.2（构建 25G83），arm64 |
| Xcode | 26.6（17F113），`ld-1267` |
| `rustc` / `cargo` | 1.95.0（Homebrew），宿主三元组 `aarch64-apple-darwin` |
| `rustup` | 缺失 —— Rust 工具链是 Homebrew formula，因此无法添加交叉目标 |
| 已安装 std 目标 | 仅 `aarch64-apple-darwin` |
| Node / npm | v24.16.0 / 11.13.0 |
| `@tauri-apps/cli` | 2.11.4（`tauri` crate 2.11.5） |
| `actionlint` | 缺失 —— 改用 YAML 解析校验工作流 |
| `gh` | 2.97.0，已安装但**未认证**（`The token in keyring is invalid`） |

`rustup` 缺失是下表 macOS x86_64 行未构建的唯一原因。这是宿主机属性，不是
仓库缺陷。

## 产物矩阵

`已构建?` 指在本宿主机上、从该基线产生了实际产物。`已签名?` 与 `已公证?`
描述的是产物当前状态，而非发布时应有的状态。

| 平台 | 架构 | 格式 | 已构建? | 已签名? | 已公证? | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| macOS | arm64 | GUI `.app` | 是 | 仅 ad-hoc —— 链接器签名的 Mach-O，bundle 未封印 | 否 | 二进制 `01373033…d6d44eca` |
| macOS | arm64 | GUI `.app` zip | 是 | 继承 `.app` | 否 | `0366e3d2…b5ed63a9` |
| macOS | arm64 | GUI `.dmg` | 是 | 容器未签名 | 否 | `8bba128a…df487da9` |
| macOS | arm64 | CLI/TUI `.tar.gz` | 是 | 仅 ad-hoc —— 链接器签名的 Mach-O | 否 | `18426464…f9eda50f` |
| macOS | x86_64 | GUI `.app` / `.dmg` | 未构建 —— 未安装 `x86_64-apple-darwin` std 且 `rustup` 缺失 | — | — | — |
| macOS | x86_64 | CLI/TUI `.tar.gz` | 未构建 —— 同一工具链缺口 | — | — | — |
| Windows | x86_64（`x86_64-pc-windows-msvc`） | CLI/TUI `.tar.gz` | 未构建 —— 无 Windows 宿主；CI 路径存在且已准备好，但其分发未获授权 | — | — | — |
| Windows | x86_64 | GUI `.msi` / NSIS `.exe` | 未构建 —— **任何平台上都没有工作流作业产出 GUI 产物** | — | — | — |
| Linux | x86_64（`x86_64-unknown-linux-gnu`） | CLI/TUI `.tar.gz` | 未构建 —— 无 Linux 宿主；CI 路径已准备，分发未获授权 | — | — | — |
| Linux | x86_64 | GUI `.deb` / `.rpm` / `.AppImage` | 未构建 —— 没有工作流作业产出 GUI 产物 | — | — | — |

完整校验和见下一节。缩写列只是为了表格可读，不是第二份事实来源。

### 三平台门禁未达成

`docs/parallel-development-plan.md` 要求 macOS、Linux、Windows 均可构建并启动，
并把 GitHub Release 与 Homebrew tap 视为同一个发布单元。按该规则，本矩阵在 CLI
上是三分之一平台，在 GUI 上也是三分之一，且 GUI 根本没有跨平台路径。R1 不能
依据本证据报告该门禁已达成。

## 产物与校验和

产物是构建输出，不进入受跟踪源码（`/target` 与 `/dist` 被忽略）。它们在工作树中
产生并复制到批次证据目录；校验和是持久记录。

`SHA256SUMS` 原文：

```
18426464fd317cffaa31b02c58810db75ff9af6e5531a703361f813cf9eda50f  viden-v0.1.30-aarch64-apple-darwin.tar.gz
0366e3d2ac8aac162806a31f4873ebc6bb5f389756973df95fb32ab7b5ed63a9  Viden-0.1.0-rc.5-aarch64-apple-darwin.app.zip
8bba128ad0eb6855c79f2802c6603e09fdbd698845952f5aa47ef784df487da9  Viden_0.1.0-rc.5_aarch64.dmg
```

| 产物 | 构建路径 | 字节数 |
| --- | --- | --- |
| GUI app bundle | `target/release/bundle/macos/Viden.app` | 3 个文件共 43,367,611 |
| GUI app zip | 对上者执行 `ditto -c -k --sequesterRsrc --keepParent` | 14,340,514 |
| GUI 磁盘镜像 | `target/release/bundle/dmg/Viden_0.1.0-rc.5_aarch64.dmg` | 15,000,970 |
| CLI/TUI 归档 | `dist/viden-v0.1.30-aarch64-apple-darwin.tar.gz` | 10,196,763 |
| 其中的 CLI/TUI 二进制 | `viden-v0.1.30-aarch64-apple-darwin/viden` | 28,282,432 |

`.app` 恰好包含三个文件 —— `Contents/Info.plist`、
`Contents/MacOS/viden-gui`（42,666,768 字节）与
`Contents/Resources/icon.icns`。前端被嵌入二进制，所以 bundle 中没有 Web 资源树。

bundle 元数据，来自 `Contents/Info.plist`：

| 键 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleVersion` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleName` | `Viden` |

打包后的 CLI 报告 `viden 0.1.30`。

## 签名状态

对 `.app` 执行 `codesign -dv --verbose=4` 的输出：

```
Identifier=viden_gui-8e75e0f205f8032b
Format=app bundle with Mach-O thin (arm64)
CodeDirectory v=20400 size=330872 flags=0x20002(adhoc,linker-signed) ...
Signature=adhoc
Info.plist=not bound
TeamIdentifier=not set
Sealed Resources=none
```

请准确理解：该签名是**链接器**对 Mach-O 的 ad-hoc 签名，而不是 bundle 签名。
`tauri.conf.json` 未声明 `bundle.macOS.signingIdentity`、entitlements 或公证
字段，所以打包器没有做任何签名。后果是：

- 代码签名标识是链接器带哈希后缀的名字，而不是 bundle 标识 `dev.viden.gui`；
- `Info.plist` 未绑定到签名且 `Sealed Resources=none`，因此
  `codesign --verify --deep --strict` 以
  `code has no resources but signature indicates they must be present` 失败；
- 在任何非构建机上，Gatekeeper 都会隔离该 `.dmg`。

这是未签名本地构建的诚实状态。真实签名需要 Developer ID 身份、在
`tauri.conf.json` 中配置 `signingIdentity` 与 entitlements，以及逐步授权。
目前三者都不存在。

公证：**未执行 —— 无凭据/无授权**。

## 可复现性

从同一源码构建两次，其间强制 `viden-gui` 重新编译并重新链接
（`cargo clean -p viden-gui`），然后比较产物树。

| 项 | 构建 1 对比构建 2 |
| --- | --- |
| `.app` 文件清单 | 相同 |
| `Contents/MacOS/viden-gui` | **逐字节相同** —— 两次均为 `013730333814628a50ff28b2d71c44f993a93807e8be0d5e9eff6e6ad6d44eca` |
| `Contents/Info.plist` | **相同** —— `d04627c8…b40fb069` |
| `Contents/Resources/icon.icns` | **相同** —— `e7cfb70c…58b93efc` |
| `.app` zip | **不同** —— `0366e3d2…` 对 `1976acbd…` |
| `.dmg` | **不同** —— `8bba128a…` 对 `98e9aab8…`，且大小相差 7 字节（15,000,970 对 15,000,963） |

每处差异的原因：

- **`.app` 内容是可复现的。** Rust 的 release 编译与链接在一次完整重编译后
  产出逐字节相同的二进制，因此在固定工具链上 bundle 本身是确定性的。这正是
  "同源同应用"真正需要的结果。
- **zip 不可复现**，因为 `ditto -c -k` 会存储每个条目的修改时间，而打包器每次
  构建都会重写 bundle（构建 1 打戳 `15:58:52`，构建 2 为 `16:00:51`）。载荷相同，
  容器时间戳不同。两次构建都导出了 `SOURCE_DATE_EPOCH`，Tauri 打包器并不遵循它。
- **`.dmg` 不可复现**，且差异远大于 zip：15,000,970 字节中有 14,906,628 字节
  不同。UDZO 磁盘镜像内嵌 HFS 卷创建日期与 UUID，它们进入每个压缩块，因此一个
  时间戳变化会级联到整个镜像。要得到逐字节稳定的 `.dmg`，需要本仓库目前没有的
  后处理步骤。

给 R1 的实用规则：发布并校验 `.app` 及其二进制的校验和，把 `.dmg` 校验和当作
单次构建的标识符，而不是可复现性证据。

## `release.yml` 实际构建什么

通过 YAML 解析校验（本宿主机未安装 `actionlint`）：

```
YAML PARSE: ok
triggers: workflow_dispatch
  input tag: required=true type=string default="v0.1.25"
  input upload_to_release: required=true type=boolean default=true
  matrix[0]: os=macos-latest target=aarch64-apple-darwin
  matrix[1]: os=macos-latest target=x86_64-apple-darwin
  matrix[2]: os=ubuntu-latest target=x86_64-unknown-linux-gnu
  matrix[3]: os=windows-latest target=x86_64-pc-windows-msvc
upload-release if: ${{ github.event.inputs.upload_to_release == 'true' }}
```

每个矩阵条目都运行 `scripts/package-release.sh "$version" "$target"`，它**只**
构建 `viden-cli`，产出 `dist/viden-v<version>-<target>.tar.gz` 及同名 `.sha256`。
`upload-release` 作业以 `upload_to_release == 'true'` 为条件，并需要
`contents: write`。

由此得出三项发现，每项都是 R1 的输入：

1. **没有任何作业构建 GUI 产物。** 工作流中没有 `npm ci`、没有 `tauri build`，
   也没有任何 `.app`/`.dmg`/`.msi`/`.deb`。今天无论何种授权级别，分发它都无法
   产出 Windows 与 Linux 的 GUI 产物。补上三平台门禁的 GUI 一半需要新增工作流
   作业，而不是一次分发。
2. **`tag` 输入是构建标签，不是检出 ref。** 工作流检出的是被分发的 ref，并通过
   去掉 `tag` 的前导 `v` 推导版本。在 `main` 上分发并传入 `tag=v0.1.30`，构建的
   是 `main`，只是标注为 `0.1.30`。
3. **0.3.5 候选不在 GitHub 上。** `f2298f4f` 不在任何远程分支上；`origin/main`
   是 `75e2a84a`。因此今天分发会构建 `main` 而不是集成分支 —— 所以这次分发只有
   在候选被推送之后才有意义，而推送本身也是需要授权的步骤。

## 已准备但未分发的分发命令

已准备、已审阅并刻意**未执行**。`upload_to_release=false` 使其成为仅构建的
校验，不产生任何发布变更。

```bash
gh workflow run release.yml \
  --repo wikieden/viden \
  --ref <已推送的候选 ref> \
  -f tag=v0.1.30 \
  -f upload_to_release=false
```

前置条件，目前全部未满足：

1. 针对分发步骤的显式"go"；
2. `gh` 已认证 —— 当前 keyring 令牌无效，命令会在到达 API 之前因认证失败；
3. 候选已推送，使 `--ref` 指向 GitHub 能检出的对象；若省略 `--ref`，工作流会在
   默认分支上运行，这正是 `scripts/release-smoke.sh --github-actions` 今天的行为。

获得"go"后的后续：用 `gh run watch` / `gh run list --workflow release.yml` 轮询，
再 `gh run download <id> --dir <证据目录>`，并把每个产物的路径、大小与校验和
记录到本文件。

## 离线检查

`scripts/release-smoke.sh --skip-package --version 0.1.30 --target
aarch64-apple-darwin` 已运行，实时供应商、GitHub Actions、GitHub Release 与
Homebrew 开关均关闭。16 项通过，6 项按开关跳过：

```
- PASS `cargo-fmt`
- PASS `tdd-testing-contract-smoke`
- PASS `tui-turn-controller-smoke`
- PASS `rc-tui-stability-contract-smoke`
- PASS `final-zero-bug-contract-smoke`
- PASS `cargo-clippy`
- PASS `viden-terminal-entry-tests`
- PASS `workspace-tests`
- PASS `tui-regression`
- PASS `fallback-cli-smoke`
- PASS `plan-mode-smoke`
- PASS `daily-loop-smoke`
- PASS `codex-app-server-protocol-fixture`
- PASS `codex-app-server-write-guard`
- PASS `lane-operator-loop-smoke`
- PASS `rc-tui-stability-smoke`
- SKIP `package-smoke`
- SKIP `deepseek-dev-scenario-smoke` (use `--deepseek`)
- SKIP `provider-live-smoke` (use `--provider-smoke <id>`)
- SKIP `github-actions-release-validation` (use `--github-actions`)
- SKIP `github-release-assets-validation` (use `--github-release-assets`)
- SKIP `homebrew-validation` (use `--homebrew`)
```

`cargo-clippy` 即 `cargo clippy --workspace --all-targets -- -D warnings`，
零警告通过；`workspace-tests` 是完整套件。`package-smoke` 被 `--skip-package`
跳过，改由上文直接运行的
`scripts/package-release.sh 0.1.30 aarch64-apple-darwin` 覆盖，其归档已在
校验和表中，二进制报告 `viden 0.1.30`。最后四个 SKIP 行是本批次未获授权运行的
对外环节。

## 带入 R1 的缺口

1. macOS x86_64 需要本宿主机上的 `rustup` 工具链，或者使用 CI 矩阵条目 ——
   该条目已存在，只需在已推送的 ref 上获得授权分发。
2. Windows 与 Linux 的 CLI 归档需要授权分发。
3. Windows 与 Linux 的 GUI 产物需要新增工作流作业；任何分发都无法产出它们。
4. 签名需要 Developer ID 身份，以及在
   `apps/gui/src-tauri/tauri.conf.json` 中配置 `signingIdentity` 与
   entitlements；公证需要凭据与 `notarytool` 步骤。
5. `.dmg` 输出不是逐字节可复现的；R1 需决定是否要求这一点。
6. GitHub Release 与 Homebrew tap 作为同一发布单元仍未验证。
