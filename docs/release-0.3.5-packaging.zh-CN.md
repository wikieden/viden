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
| Windows | x86_64 | GUI `.msi` / NSIS `.exe` | 未构建 —— 无 Windows 宿主机；批次 P2 已在下文新增 `gui-bundle` 作业，但其分发未获授权 | — | — | — |
| Linux | x86_64（`x86_64-unknown-linux-gnu`） | CLI/TUI `.tar.gz` | 未构建 —— 无 Linux 宿主；CI 路径已准备，分发未获授权 | — | — | — |
| Linux | x86_64 | GUI `.deb` / `.AppImage` | 未构建 —— 无 Linux 宿主机；批次 P2 已在下文新增 `gui-bundle` 作业，但其分发未获授权。`.rpm` 不在该作业的产物列表内 | — | — | — |

完整校验和见下一节。缩写列只是为了表格可读，不是第二份事实来源。

### 三平台门禁未达成

`docs/parallel-development-plan.md` 要求 macOS、Linux、Windows 均可构建并启动，
并把 GitHub Release 与 Homebrew tap 视为同一个发布单元。按该规则，本矩阵在 CLI
上是三分之一平台，在 GUI 上也是三分之一。R1 不能依据本证据报告该门禁已达成。
批次 P2 为 GUI 补上了跨平台 CI 路径 —— 见 [GUI 工作流（P2）](#gui-工作流p2)
—— 但路径不等于构建：在获得授权的分发真正产出产物之前，对应平台上没有任何产物。

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
   作业，而不是一次分发。**已由批次 P2 于 2026-09-13 关闭**；下一节记录了关闭
   它的那个作业。
2. **`tag` 输入是构建标签，不是检出 ref。** 工作流检出的是被分发的 ref，并通过
   去掉 `tag` 的前导 `v` 推导版本。在 `main` 上分发并传入 `tag=v0.1.30`，构建的
   是 `main`，只是标注为 `0.1.30`。
3. **0.3.5 候选不在 GitHub 上。** `f2298f4f` 不在任何远程分支上；`origin/main`
   是 `75e2a84a`。因此今天分发会构建 `main` 而不是集成分支 —— 所以这次分发只有
   在候选被推送之后才有意义，而推送本身也是需要授权的步骤。

## GUI 工作流（P2）

批次 P2 关闭了上文的发现 1。`release.yml` 在保持 CLI `build` 矩阵不变的前提下
新增了 `gui-bundle` 矩阵，`scripts/release-smoke.sh --github-release-assets`
也认识了 GUI 产物名。没有任何分发、打标签、签名、公证、上传、合并或推送。

| 字段 | 值 |
| --- | --- |
| 分支 | `claude/release-workflow-p2` |
| 工作树 | `.worktrees/release-workflow-p2` |
| 基线 | `claude/int-0.3.5` 的 `e3f75e0d` |
| 改动文件 | [`.github/workflows/release.yml`](../.github/workflows/release.yml)、[`scripts/release-smoke.sh`](../scripts/release-smoke.sh) |

### 作业

| 作业 | 运行于 | needs | 条件 |
| --- | --- | --- | --- |
| `build` | 矩阵 `os` | — | 始终运行；未改动，仍然只构建 `viden-cli` |
| `gui-bundle` | 矩阵 `os` | — | 始终运行；`fail-fast: false`，一个平台失败不会取消其余平台 |
| `upload-release` | `ubuntu-latest` | `[build, gui-bundle]` | `github.event.inputs.upload_to_release == 'true'` |

`upload-release` 原先只依赖 `build`，现在同时等待两个矩阵：一次发布要么同时拿到
CLI 归档与 GUI 产物，要么什么都拿不到。

### `gui-bundle` 步骤

| # | 步骤 | 条件 | 作用 |
| --- | --- | --- | --- |
| 0 | `actions/checkout@v6` | — | 检出被分发的 ref |
| 1 | `dtolnay/rust-toolchain@stable` | — | 带 `targets: <矩阵 target>` |
| 2 | `actions/setup-node@v6` | — | Node 24，npm 缓存键为 `apps/gui/package-lock.json` |
| 3 | 安装 Linux 打包依赖 | `runner.os == 'Linux'` | `libwebkit2gtk-4.1-dev`、`build-essential`、`curl`、`wget`、`file`、`libxdo-dev`、`libssl-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf` |
| 4 | 读取 GUI 版本 | — | 对 `apps/gui/src-tauri/tauri.conf.json` 执行 `node -p`，写入 `gui` 步骤输出 |
| 5 | 安装前端依赖 | — | `npm --prefix apps/gui ci` |
| 6 | 探测 macOS 签名密钥 | `runner.os == 'macOS'` | 只输出 `sign` 与 `notarize` 两个布尔值 |
| 7 | 构建 GUI 产物 | — | `npm --prefix apps/gui run tauri -- build --target <target> --bundles <bundles>` |
| 8 | 收集产物 | — | macOS 上对 `.app` 执行 `ditto` 压缩，其余平台直接复制，统一放入 `dist/gui/<target>/` |
| 9 | 计算校验和 | — | 写出 `SHA256SUMS-gui-<target>` |
| 10 | 写作业摘要 | `always()` | 版本、产物列表、签名状态、校验和块 |
| 11 | `actions/upload-artifact@v7` | — | `if-no-files-found: error` |

产物携带的版本来自被分发 ref 的 `tauri.conf.json`，绝不来自 `tag` 输入。两条
版本线各自独立 —— CLI 归档标注为 `v0.1.30`，而 GUI 是 `0.1.0-rc.5` —— 而且 P1
的发现 2 已经说明 `tag` 是构建标签而不是检出 ref。

### 各 target 的产物

产物名中的版本取自步骤 4 读到的 GUI 版本，此处写作 `<version>`。

| Target | Runner | `--bundles` | Artifact | 其中的文件 |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `macos-latest` | `app,dmg` | `viden-gui-<version>-aarch64-apple-darwin` | `Viden-<version>-aarch64-apple-darwin.app.zip`、`Viden_<version>_aarch64.dmg`、`SHA256SUMS-gui-aarch64-apple-darwin` |
| `x86_64-apple-darwin` | `macos-latest` | `app,dmg` | `viden-gui-<version>-x86_64-apple-darwin` | `Viden-<version>-x86_64-apple-darwin.app.zip`、`Viden_<version>_x64.dmg`、`SHA256SUMS-gui-x86_64-apple-darwin` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `msi,nsis` | `viden-gui-<version>-x86_64-pc-windows-msvc` | `Viden_<version>_x64_en-US.msi`、`Viden_<version>_x64-setup.exe`、`SHA256SUMS-gui-x86_64-pc-windows-msvc` |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `deb,appimage` | `viden-gui-<version>-x86_64-unknown-linux-gnu` | `Viden_<version>_amd64.deb`、`Viden_<version>_amd64.AppImage`、`SHA256SUMS-gui-x86_64-unknown-linux-gnu` |

只有第一行的名字是实测得到的，来自下文的本地预演。其余三行是 Tauri v2 文档中的
命名约定，在真正分发之前仍未经验证。作业本身并不依赖这些名字：它对每个打包器的
输出目录做通配，而不是拼接文件名，因此 Tauri 改名只会让上表过时，不会让构建失败。

`.app` 是目录，不能作为发布资产，因此以 `ditto -c -k --sequesterRsrc --keepParent`
的压缩包形式发布。Linux 产物列表刻意不含 `.rpm`；是否加入是 R1 的决定，不是 P2 的。

### 签名门禁

默认关闭。步骤 6 读取六个 Apple 密钥并给出两个布尔值：

- 只有 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、
  `APPLE_SIGNING_IDENTITY` 全部存在时，`sign` 才为真；
- 只有 `sign` 为真且 `APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID` 也存在时，
  `notarize` 才为真。

步骤 7 仅在对应布尔值为真时通过表达式拿到各项凭据，否则替换为空字符串。任何密钥
都不会被打印、写入文件或导出到 `$GITHUB_ENV`，因此作业中后续步骤也读不到。密钥
缺失时构建只会产出 ad-hoc 产物，作业摘要会照实写明这一点，并指出 Gatekeeper 会
隔离它 —— 这正是 P1 为本地构建手工记录的状态。

真正的签名还需要在 `apps/gui/src-tauri/tauri.conf.json` 中配置
`bundle.macOS.signingIdentity` 与 entitlements。该文件属于 GUI 轨道，P2 未触碰，
因此签名通路已接好但尚未配置；P1 的缺口 4 仍然开着。

### 带入工作流的可复现性结论

P1 的发现 3：`.app` 压缩包与 `.dmg` 都不是逐字节可复现的，因为 `ditto` 会保存每个
条目的修改时间，而 UDZO 镜像内嵌卷创建日期与 UUID。`.app` 中的 Mach-O 是可复现的。

因此 macOS 上的 `SHA256SUMS-gui-<target>` 有三行 —— 压缩包、`.dmg`，以及一行名为
`Viden.app/Contents/MacOS/viden-gui` 的记录。前两者是每次构建的标识符，第三者才是
可跨构建比较的构建身份。用于计算它的临时副本会在上传前删除，因此该二进制本身不会
成为发布资产。

预演暴露出一处限定：二进制摘要只在工具链**与构建路径**都固定时才稳定。P1 在
`.worktrees/release-packaging-p1` 中的两次构建都得到 `01373033…d6d44eca`；P2 在
另一个工作树中、并且带 `--target` 的预演得到
`74f1b268ee51b2aa7c80f9f18e2229c560e2fd072b1a5de47e7cef74b3f5eb12`。不加
`--remap-path-prefix` 时，跨不同根目录的检出比较摘要没有意义；比较同一作业的两次
运行才有意义。

### Windows MSI 的预发布版本限制

Tauri 的 MSI 目标只接受单段纯数字的 semver 预发布标识。GUI 版本是 `0.1.0-rc.5`，
其预发布段是 `rc.5`，而 `apps/gui/src-tauri/tauri.conf.json` 未声明任何
`bundle.windows.wix` 版本覆盖。因此在当前版本下分发时，Windows 行的 `msi` 一半
预计会失败；`nsis` 没有这条规则。

两条出路都在 P2 的写入范围之外：让 GUI 以不带预发布段的 `0.1.0` 发布 —— 计划已把
它列为生产门禁通过时的结果 —— 或者在 `tauri.conf.json` 中给 WiX 打包器一个
`major.minor.patch` 版本覆盖，而该文件属于 GUI 轨道。这是依据打包器文档规则做出的
预测，不是观测结果 —— 没有任何 Windows 构建运行过。

### 已执行的验证

没有分发，因此下列每一条要么是本地观测，要么是一次解析。

- **YAML 解析。** 本宿主机上仍无 `actionlint`，因此与 P1 一样解析工作流并打印其
  结构。`YAML PARSE: ok`，三个作业，两个矩阵各四条，逐步骤列出 `if` 与 `env`。
- **干审阅表。** 上文两张表就是该审阅，由解析结果生成而非手工誊抄。
- **`tauri build` 预演。** `npm --prefix apps/gui ci && npm --prefix apps/gui run
  tauri -- build --target aarch64-apple-darwin` 在本宿主机的 P2 工作树中运行并以
  0 退出：`added 97 packages`、`vite v7.3.6 … built in 454ms`、
  ``Finished `release` profile … in 1m 23s``，随后 `Finished 2 bundles at:` 指向
  `target/aarch64-apple-darwin/release/bundle/` 下的 `.app` 与
  `Viden_0.1.0-rc.5_aarch64.dmg`。由此得到两个事实：传入 `--target` 时打包器写在
  target 三元组目录下，这正是收集步骤所通配的位置；macOS 的默认产物集合恰好就是
  `app,dmg`，因此作业中的 `--bundles app,dmg` 选中的正是预演产出的那一组。
- **收集与校验和预演。** 步骤 8 与步骤 9 原样对上述产物运行，产出
  `Viden-0.1.0-rc.5-aarch64-apple-darwin.app.zip`（14,348,908 字节）、
  `Viden_0.1.0-rc.5_aarch64.dmg`（15,008,320 字节），以及：

```
691403d07f9cde26b65293e09b18fc1d796c28ac41e1294696db6877dec7b0af  Viden-0.1.0-rc.5-aarch64-apple-darwin.app.zip
c096fe08e7e241f7b4774c9ee753ef2f8e856501a7e850313acf8a1512ec9452  Viden_0.1.0-rc.5_aarch64.dmg
74f1b268ee51b2aa7c80f9f18e2229c560e2fd072b1a5de47e7cef74b3f5eb12  Viden.app/Contents/MacOS/viden-gui
```

- **`--target` 下的 ad-hoc 状态已确认。** 对预演产物执行 `codesign -dv` 报告
  `flags=0x20002(adhoc,linker-signed)`、`Signature=adhoc`、
  `TeamIdentifier=not set`、`Sealed Resources=none` —— 正是作业摘要未签名分支所
  描述的状态。
- **产物名期望。** 更新后的 `github_release_asset_validation` 对六份合成的
  release 负载做了验证：完整的 `0.3.5` 资产集合通过；仅含 CLI 的 release 仍然
  通过，因此该检查不会打断 GUI 作业出现之前所做的发布；有 GUI 产物却没有任何
  `SHA256SUMS-gui-*`、有 `SHA256SUMS-gui-*` 却没有任何 GUI 产物、出现无法识别的
  资产名、以及 `.tar.gz` 缺少对应 `.sha256`，四种情况各自以专属消息失败。
- **Linux 依赖列表。** 取自 Tauri v2 的先决条件与该项目自己发布的 GitHub Actions
  指引。在这台 macOS 宿主机上无法预演；该行列在下文的未证明项中。

### 未执行的分发

与 P1 准备的命令相同 —— 同一个工作流，GUI 矩阵在同一次分发下运行：

```bash
gh workflow run release.yml \
  --repo wikieden/viden \
  --ref <已推送的候选 ref> \
  -f tag=v0.1.30 \
  -f upload_to_release=false
```

**未执行。** 其前置条件就是 P1 的三条 —— 明确的“go”、已认证的 `gh`、已推送的候选
ref —— P2 没有新增任何一条。P2 未获授权分发、打标签、发布、签名、公证或改动 `gh`
认证，也没有做其中任何一件。

只有分发才能证明的事项：

| 断言 | 不分发能否证明？ |
| --- | --- |
| macOS arm64 的 `app` + `dmg` 构建以及收集/校验和步骤可用 | 能 —— 上文已预演 |
| macOS x86_64 可构建 | 不能 —— 本宿主机缺 `rustup`，无法添加该 target |
| Windows `nsis` 可构建 | 不能 |
| Windows `msi` 可构建 | 不能，且上文的预发布规则预测它在 `0.1.0-rc.5` 下会失败 |
| Linux `deb` + `appimage` 可构建，且 apt 列表足够 | 不能 |
| `actions/setup-node@v6` 与 npm 缓存键在 runner 上可解析 | 不能 |
| 各产物汇入 `upload-release` 时不会重名 | 不能 —— 名字在构造上唯一，但只有真正运行才能证明 |
| 密钥缺失时签名门禁干净跳过 | 不能 —— 表达式经审阅是正确的，而且仓库也没有 Apple 密钥可用来测试为真的分支 |

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
3. Windows 与 Linux 的 GUI 产物现在已有工作流作业（上文批次 P2）；它们需要在
   已推送的 ref 上获得授权分发，其中 Windows 的 `.msi` 一半还需要一个不带
   `rc.N` 预发布段的版本，或者一个 WiX 版本覆盖。
4. 签名需要 Developer ID 身份，以及在
   `apps/gui/src-tauri/tauri.conf.json` 中配置 `signingIdentity` 与
   entitlements；公证需要凭据与 `notarytool` 步骤。
5. `.dmg` 输出不是逐字节可复现的；R1 需决定是否要求这一点。
6. GitHub Release 与 Homebrew tap 作为同一发布单元仍未验证。

## R1 —— rc.6 版本提升、重建的产物，以及 P1 与 P2 留下的决定

由批次 R1（生产发布门禁）于 2026-10-10 在 `claude/release-gate-r1` 上新增，基线为
`claude/int-0.3.5` 的 `d522e4f38f7643d0e1dd37d70af5e84c43f59765`，它同时也是 Core
`0.3.8` 契约 checkpoint。上文[来源](#来源)中的版本是 P1 的，保持原样；本节记录 R1
改动与重建的内容。

### 重建产物所携带的版本

| 产品线 | 版本 |
| --- | --- |
| Workspace / CLI | `0.1.30`（未变；见下文的发布身份发现） |
| Core | `0.3.8`，checkpoint `d522e4f3` |
| TUI | `0.3.6` |
| GUI | `0.1.0-rc.6`（`tauri.conf.json`、`package.json`、`src-tauri/Cargo.toml`、`release-manifest.toml`、冻结的 `manifests/0.1.0-rc.6.toml`） |

### Windows MSI 版本映射

[Windows MSI 的预发布版本限制](#windows-msi-的预发布版本限制)中的预测已被落实，而不是
留给一次失败的分发：`apps/gui/src-tauri/tauri.conf.json` 现在声明
`bundle.windows.wix.version = "0.1.0"`。映射如下：

| 产物 | 携带的版本 |
| --- | --- |
| macOS `.app` / `.dmg`、Windows NSIS `-setup.exe`、Linux `.deb` / `.AppImage` | `0.1.0-rc.6` |
| Windows `.msi`（WiX `ProductVersion`） | `0.1.0` |

因此由 rc.6 构建的 `.msi` 与日后由提升后的 `0.1.0` 构建的 `.msi` 携带相同的 WiX 版本，
Windows Installer 会把后者视为同一产品版本而不是升级。对于一个不经由 MSI 渠道分发的
候选版本，这是可以接受的；若提升到 `0.1.0`，就移除该覆盖（此时这个三段式版本本身就是
版本），或者为下一个候选改为 `0.1.1`。`tests/architecture_boundary.rs` 断言了该覆盖，
因此移除它必须是一次刻意的修改。打包器会把两个版本中的哪一个写进 `.msi` 文件名，在
Windows 分发运行之前仍未验证；上传的 glob 不依赖于此。

### 恢复 rc.5 快照

DS2 的 token 改动把 `tokens.css` 摘要从 `826826ee…` 移到 `2d80baa9…`，并且由于边界测试
当时断言当前 manifest 与 `manifests/0.1.0-rc.5.toml` 相等，它连已发布的 rc.5 快照也一并
改了。R1 把该文件逐字节恢复为
`git show bcf3d64c:apps/gui/manifests/0.1.0-rc.5.toml`（SHA-256
`b2177792cfd3a5f699b9bde57f22d7d5d0c37db74ba2c64a3c9aad4feeae7092`，
`token_revision_sha256 = 826826ee…`），把新摘要写进新的冻结快照
`manifests/0.1.0-rc.6.toml`；边界测试现在把当前 manifest 与 rc.6 快照比较，并钉住 rc.5
快照自身的 SHA-256。

### 重建的 macOS 产物

`npm --prefix apps/gui ci` 之后执行 `npm --prefix apps/gui run tauri -- build
--bundles app`，56 秒内以退出码 0 完成（``Finished `release` profile [optimized]
target(s) in 50.85s``），环境为 macOS 27.0.1 (26A434)、Xcode 27.0 (27A266a)、`rustc`
1.95.0（Homebrew）、Node v24.16.0 / npm 11.13.0。`rustup` 仍然缺席，因此 x86_64 仍只能
由 CI 构建。

| 事实 | 值 |
| --- | --- |
| 路径 | `target/release/bundle/macos/Viden.app`（位于 R1 worktree 中） |
| `CFBundleShortVersionString` / `CFBundleVersion` | `0.1.0-rc.6` / `0.1.0-rc.6` |
| `CFBundleIdentifier` / `CFBundleExecutable` | `dev.viden.gui` / `viden-gui` |
| 文件 | 3 个 —— `Info.plist` 964 B、`MacOS/viden-gui` 42,877,136 B、`Resources/icon.icns` 699,879 B（合计 43,577,979 B） |
| 架构 | `Mach-O 64-bit executable arm64` |
| **二进制 SHA-256（构建身份）** | `efff125904239b9b7c7e4cf687a59af6278dda592a258651ac2021126d31c53a` |
| `Info.plist` SHA-256 | `1585343fecb1ea88dc01524f0044c55fdfebbb306fe9bf488679ddc8ab1b8aa1` |
| `icon.icns` SHA-256 | `e7cfb70caf2180b0f7173aad3adb97938e270de78b7427d7d1d0f93258b93efc`（自 P1 起未变） |
| `.app` zip（`ditto -c -k --sequesterRsrc --keepParent`，单次构建标识） | 14,430,604 B，`e95b97fb141d2044cc97eb702cffcfdf90feb9970f21ec5a3db5b01f04c04dd6` |
| 签名 | ad-hoc，`flags=0x20002(adhoc,linker-signed)`、`Info.plist=not bound`、`TeamIdentifier=not set`、`Sealed Resources=none` |

R1 没有构建 `.dmg`：任务要求的重建是 `--bundles app`，而 `.dmg` 校验和只是单次构建的
标识，不是证据（见下文）。该产物没有被启动。

### R1 记录的决定

1. **构建身份是 `.app` 二进制的 SHA-256。** `.dmg` 与 `.app` zip 不能逐字节复现
   （P1）；它们的校验和只标识一次构建，随产物一同发布，但从不跨构建比较。P2 新增的
   `SHA256SUMS-gui-<target>` 中指名 `Viden.app/Contents/MacOS/viden-gui` 的那一行才是
   用来比较的。
2. **二进制摘要只在同一构建路径内比较。** P1 与 P2 已表明二进制摘要会随 checkout 的
   绝对路径（以及 `--target`）而变化。R1 不添加 `--remap-path-prefix`；因此摘要只在同一
   CI 作业的两次运行之间、或同一 worktree 的两次构建之间可比，而不能在本地构建
   （`efff1259…`）与 CI 构建之间比较。
3. **`0.3.5` 的 Linux 产物列表继续不含 `.rpm`。** 计划中的 Linux 行是 `.deb` 与
   `.AppImage`；加入 `.rpm` 会给一个 Linux 行一次都没跑过的矩阵增加一种此处没有任何宿主机
   能构建或启动的格式。若有 Fedora/RHEL 用户提出需求，它是 `0.3.6` 的候选项。

### 门禁自身无法解决的发布身份发现

2026-10-10 的只读 `gh` 调用（`gh auth status` 现已以 `wikieden` 身份认证，具备 `repo`
与 `workflow` scope —— P1 记录的「未认证」已不再成立）：

- 最新的 GitHub Release 是 **`v0.1.30`，标题为 "RoboCode v0.1.30"**，发布于
  2026-06-11，其资产为 `robocode-v0.1.30-<target>.tar.gz` 及 `.sha256`；
- `wikieden/homebrew-tap` 只有 `Formula/robocode.rb`，其 URL 指向
  `github.com/wikieden/robocode/releases/download/v0.1.30/robocode-v0.1.30-…`；
- `origin/main` 位于 `0dcfaa66`；没有任何 `release.yml` 运行使用过 `0.3.5` 的 ref。

`scripts/release-gate.sh` 接收的是 workspace/CLI 版本（`0.1.30`），其 postpublish 环节
期望资产以 `viden-v` / `Viden` 为前缀，并期望存在 `wikieden/tap/viden` formula。对照上述
状态，**在 `0.1.30` 上运行 postpublish 两个环节都会失败** —— 该 tag 已以旧名称存在，
而 tap 中没有 `viden` formula。因此 `0.3.5` 发布需要先做一个版本决定（新的 workspace
版本，例如 `0.1.31`，或为三条产品线采用另一种 tag 方案），以及一次新增或重命名 formula
的 tap 改动。两者都是用户的决定，也都是对外变更；R1 两者都不做。

### Prepublish 门禁

`scripts/release-gate.sh --version 0.1.30 --phase prepublish` **拒绝运行**，退出码 2：
`DEEPSEEK_API_KEY is required for release prepublish gate`。密钥不存在，也没有收到
「go」，因此门禁整体没有运行。它的 `--dry-run` 列出七条命令；随后在 `a9d8e51c`（声明
checkpoint 的提交）处按该顺序手动运行了所有既不需要密钥、也不涉及对外变更的环节，证据位于
批次的 scratch 目录下：

| 环节，按 `release-gate.sh` 顺序 | 是否运行 | 结果 |
| --- | --- | --- |
| `scripts/check-task10-guards-test.sh` | 是 | 通过，`Task 10 guard fixture tests passed.` |
| `scripts/check-dependency-boundaries.sh` | 是 | 通过 |
| `scripts/check-doc-pairs.sh`，覆盖门禁指定的十二个文件 | 是 | 通过 |
| `scripts/check-doc-links.sh`，覆盖门禁指定的四个文件 | 是 | 通过 |
| `scripts/context-engine-benchmark.sh` 确定性，3 次运行 | 是 | 通过，`Result: passed`，输入 token 中位数 off `1000` / on `750`（减少 `25.00%`） |
| `scripts/release-smoke.sh --version 0.1.30 --target aarch64-apple-darwin`，不带 `--deepseek` | 是，393 秒 | **17 通过，5 跳过**：`cargo-fmt`、`tdd-testing-contract-smoke`、`tui-turn-controller-smoke`、`rc-tui-stability-contract-smoke`、`final-zero-bug-contract-smoke`、`cargo-clippy`（`-D warnings`）、`viden-terminal-entry-tests`、`workspace-tests`（串行）、`tui-regression`、`fallback-cli-smoke`、`plan-mode-smoke`、`daily-loop-smoke`、`codex-app-server-protocol-fixture`、`codex-app-server-write-guard`、`lane-operator-loop-smoke`、`rc-tui-stability-smoke`、`package-smoke`（`viden-v0.1.30-aarch64-apple-darwin.tar.gz`，10,268,799 B，SHA-256 `ef9e3fa32c41fa781e162c80240a542c9fd103f6cb21e24e7b8665e6e13e3182`，二进制报告 `viden 0.1.30`） |
| ↳ `deepseek-dev-scenario-smoke`（门禁会传 `--deepseek`） | **拒绝** | 未运行 —— 缺少授权：没有 `DEEPSEEK_API_KEY`，没有「go」 |
| ↳ `provider-live-smoke`、`github-actions-release-validation`、`github-release-assets-validation`、`homebrew-validation` | 按参数跳过 | 不属于本门禁的 prepublish 环节；后三项是未获授权的对外步骤 |
| `scripts/final-zero-bug-smoke.sh`（因版本为 `0.1.30` 而必需） | 是 | **失败，退出码 2**：`VIDEN_TUI_MANUAL_EVIDENCE_DIR is required for the final zero-bug gate`。该环节要求至少一张由人拍摄的 macOS Terminal 与一张 iTerm2 的 TUI 截图；本候选没有，而 agent 无法在锁定的屏幕上拍摄。这是证据缺口，不是授权缺口 |
| `scripts/context-engine-benchmark.sh --provider deepseek`（live A/B） | **拒绝** | 未运行 —— 缺少授权：没有 `DEEPSEEK_API_KEY`，没有「go」 |
| 迁移（契约的先迁移后重放测试） | 是 | 通过，1 个测试，且包含在两次工作区运行中 |
| 完整工作区 | 是 | 两次通过：并行（2343 通过）与 `release-smoke` 内的串行 |

因此 prepublish 门禁**没有通过**：两个环节因缺少授权被拒绝，一个环节因缺少人工证据而失败。
其余每个环节都通过。

### 带入的缺口与 R1 的处置

| P1 缺口 | R1 之后 |
| --- | --- |
| 1. macOS x86_64 | 未变 —— 需要在已推送的 ref 上获得授权分发 |
| 2. Windows/Linux CLI 归档 | 未变 —— 同上 |
| 3. Windows/Linux GUI 产物 | `.msi` 版本阻塞已解除（WiX 覆盖）；仍需在已推送的 ref 上获得授权分发 |
| 4. 签名与公证 | 未变 —— 没有身份、没有凭据、没有授权 |
| 5. `.dmg` 可复现性 | 已决定：不要求；二进制 SHA-256 即身份 |
| 6. GitHub Release + Homebrew 作为同一单元 | 未验证，且现已知需要先做版本决定与 tap formula 改动（见上文） |
