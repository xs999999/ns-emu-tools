# Citron Stable/Nightly 分支拆分计划

## 背景

当前 Yuzu 页面把 Citron Neo 作为单一分支 `citron` 展示和处理，而 Citron Neo 模拟器自身的更新检查已经把 Stable 与 Nightly 作为两个渠道：

- Stable release 源：`https://api.github.com/repos/citron-neo/emulator/releases`
- Nightly release 源：`https://api.github.com/repos/citron-neo/CI/releases`

本计划目标是把现有下拉列表里的 “Citron” 拆成 “Citron Stable” 与 “Citron Nightly” 两项，交互方式参考 Ryujinx 的 Mainline/Canary 分支。

## 目标

- 前端 Yuzu 页面下拉列表展示独立的 `Citron Stable` 与 `Citron Nightly`。
- 后端分支参数支持稳定的内部值，例如 `citron-stable` 与 `citron-nightly`。
- 旧配置中的 `citron` 保持兼容，默认迁移或等价映射到 `citron-stable`。
- 获取版本、安装、变更日志、检测当前安装版本和分支时，都能正确区分两个 Citron 渠道。
- 现有 Eden 行为不受影响。

## 设计

### 分支命名

新增 Yuzu 分支值：

- `citron-stable`：对应 Citron Neo Stable。
- `citron-nightly`：对应 Citron Neo Nightly。

兼容旧值：

- `citron` 继续被后端接受，按 `citron-stable` 处理。
- 配置加载或分支切换时可逐步把 `citron` 写回为 `citron-stable`，避免旧用户下拉框无选中项。

### Release 数据源

在 `src-tauri/src/repositories/yuzu.rs` 中拆分 Citron release API：

- Stable 使用 GitHub Releases API，走 `request_github_api` 和 `ReleaseInfo::from_github_api`。
- Nightly 使用 GitHub Releases API，走 `request_github_api` 和 `ReleaseInfo::from_github_api`。
- Nightly 的版本号不使用平台 tag（例如 `nightly-windows`），而是从 `body` / `name` / 资产名中提取 `Citron Upstream Commit` 的短 SHA，例如 `fab192f`。同一个 upstream SHA 下的 Windows、macOS、Linux release 资产需要合并成同一个版本条目，安装时再按当前平台筛选资产。
- 上游 Citron Neo updater 源码曾出现 `https://git.citron-neo.org/api/v1/repos/Citron/Emulator/releases` 这种 Forgejo/Gitea 风格 API，但当前环境无法解析该域名；本工具应优先使用当前可访问且现有代码已在使用的 GitHub API。

需要更新的入口：

- `get_all_yuzu_release_versions(branch)`
- `get_yuzu_all_release_info(branch)`
- `get_yuzu_release_info_by_version(version, branch)`
- `get_latest_change_log(branch)`
- unsupported branch 错误文案和测试

### 安装与资源选择

在 `src-tauri/src/services/yuzu.rs` 中统一分支判断：

- 所有 Citron 变体共用安装路径、可执行文件名、用户目录识别和 macOS bundle 校验逻辑。
- 资源选择不应依赖资产名中的 `stable` 或 `nightly` 文本来判断渠道。当前 `citron-neo/emulator` 的 tagged release 源虽然作为 Stable 使用，但桌面资产名仍可能是 `Citron-windows-nightly-...`、`Citron-macOS-nightly-...`、`citron_nightly-...AppImage`。
- Stable/Nightly 渠道由用户选择的分支值和对应 release API 决定；资产名只用于筛选平台、架构、包格式和工具链。
- Windows 继续优先选择非 PGO 的 `.zip`，并沿用现有 Citron 偏好 `x64-msvc`、再回退 `x64-clangtron` 的策略。
- macOS 继续选择 `.dmg`，并排除 `.zsync` 等增量更新文件。
- Linux 当前仍按既有支持策略处理 AppImage，不把文件名中的 `nightly` 当作渠道判断条件。
- 如果当前逻辑只判断 `branch == "citron"`，改成辅助函数，例如 `is_citron_branch(branch)`。
- 安装完成写入配置时保存具体分支值，优先保存用户选择的 `citron-stable` 或 `citron-nightly`。

### 版本和分支检测

当前检测逻辑已经能从窗口标题、版本字符串或可执行文件元数据识别 Citron。需要补齐：

- 检测到 Citron 但无法确定渠道时，默认返回 `citron-stable`，与旧 `citron` 兼容语义保持一致。
- 不应仅凭安装包或资产名包含 `nightly` 就判定为 `citron-nightly`，因为 Stable 源的 tagged release 资产名也可能带 `nightly`。
- 如果版本文件、配置或明确元数据能证明来自 Stable 源，返回 `citron-stable`。
- 如果明确来自 CI/nightly 源，或 release/tag 元数据指向 `citron-neo/CI`，返回 `citron-nightly`。
- 对本地已安装 Citron 但无法可靠判断渠道的情况，默认返回 `citron-stable`。
- 旧的 `citron` 检测结果在写入配置前规范化为 `citron-stable`。

### 前端

在 `frontend/src/pages/yuzu.vue` 中更新 `branches`：

- `Eden`
- `Citron Stable`
- `Citron Nightly`
- 已关闭的 Yuzu 主线/EA 保持原状

`branchMap`、`selectedEmulatorName`、版本加载、安装和切换逻辑继续使用分支 value 传给 Tauri。初始化时如果配置里读到旧值 `citron`，前端也应显示为 `citron-stable`，避免空选中。

相关类型和默认配置可同步检查：

- `frontend/src/types/index.ts`
- `frontend/src/types/DefaultConfig.ts`
- `frontend/src/utils/tauri.ts`

## 实施步骤

1. 增加 Rust 侧分支常量和规范化函数，例如 `normalize_yuzu_branch`、`is_citron_branch`、`is_citron_stable_branch`、`is_citron_nightly_branch`。
2. 拆分 `repositories/yuzu.rs` 中 Citron Stable/Nightly 的 release API 与解析函数。
3. 调整 `services/yuzu.rs` 中所有 Citron 分支判断，让安装、资源筛选、检测和配置写入使用规范化分支。
4. 调整 `commands/yuzu.rs`，确保 `switch_yuzu_branch`、安装和版本查询都接受新分支值。
5. 更新 `frontend/src/pages/yuzu.vue` 下拉列表和旧 `citron` 映射。
6. 增加或更新 Rust 单元测试，覆盖分支规范化、release 解析、资产选择和旧 `citron` 兼容。
7. 如前端有测试或类型检查脚本，补跑对应校验。

## 验证

修改 Rust 后按仓库要求执行：

- `cargo fmt`
- `cargo check`
- Windows target 的 `cargo check`
- 如果从 Windows 验证 macOS target，使用 `cargo zigbuild`

功能验证项：

- Yuzu 页面下拉框能看到 `Citron Stable` 与 `Citron Nightly`。
- 两个 Citron 分支分别加载不同 release 源的版本列表。
- `citron-nightly` 版本列表显示 upstream 短 SHA，例如 `fab192f`，而不是 `nightly-windows` 这类平台 tag。
- 选择任一 Citron 分支后安装时下载所选 release 源中的平台匹配资产，即使 Stable 源资产名包含 `nightly` 也不应被排除。
- 旧配置 `branch: "citron"` 打开后仍可正常加载，并落到 Stable 兼容路径。
- Eden 安装和版本列表不受影响。

纯后端端到端验证项：

- 为 `citron-stable` 和 `citron-nightly` 各跑一次版本列表获取，确认返回非空列表，且请求源分别是 `citron-neo/emulator` 与 `citron-neo/CI`。
- 确认 `citron-nightly` 返回的版本值是 7 位 upstream 短 SHA，并且用该 SHA 继续调用 release 详情能反查到对应 release 与资产。
- 对两个分支各取最新版本调用 release 详情接口，确认能解析 release 元数据和资产列表。
- 对两个分支各跑一次下载 URL 解析或安装前校验流程，确认能选出当前平台可用资产；Stable 源资产名包含 `nightly` 时也必须通过。
- 通过 Tauri command 层或 Rust 集成测试调用 `get_all_yuzu_versions` / `install_yuzu_by_version` 的后端路径，覆盖从分支参数进入到 repository/service 的完整链路。
- 如果真实安装会写入本机模拟器目录，端到端测试应使用临时目录或只验证到下载 URL 选择阶段，避免污染用户现有安装。

## 风险

- Stable 与 Nightly 都是 GitHub Releases API，字段形态一致，核心字段为 `tag_name`、`name`、`body`、`published_at`、`assets[].name`、`assets[].browser_download_url`。风险不在 JSON schema，而在两个仓库的发布策略和资产命名是否持续稳定。
- Nightly 的可选版本依赖 `Citron Upstream Commit` 或资产名中的 upstream SHA；如果 CI release 文案或资产命名规则变化，可能需要更新 SHA 提取逻辑。
- 上游源码里保留过 `git.citron-neo.org/api/v1/...` 的 Stable URL，但该域名当前不可解析；不要把它作为本工具的默认 Stable 源，除非后续确认官方恢复该域名。
- Stable 源资产名目前也带 `nightly`，因此资产名不能作为渠道判定依据；只能作为平台、架构、格式和工具链筛选依据。
- 旧用户配置、历史路径和检测结果如果仍写入 `citron`，前端可能出现下拉框选中状态不一致，需要统一规范化。
