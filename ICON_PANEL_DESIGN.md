# ZeroLaunch-rs 图标面板（Rolan 风格）设计文档

> **状态**：设计已定稿并经用户逐节确认（2026-10-13）。
> **下一步**：从「第 9 节 分阶段计划」的阶段 0 开始实施。
> **基线**：fork `HeiShuXianSheng/ZeroLaunch-rs`，HEAD `fef2354`（v1.4.1，2026-10-12）。
> 本文档中所有 `文件:行号` 锚点均基于该 commit 核对过。

**给下一个 AI 会话的交接说明**：这是一份完整的设计文档，不需要重新调研。开工前建议按序读：
1. 本文档第 3 节（现状调研结论）——省掉一整轮代码勘察
2. `.omp/AGENTS.md`（项目架构导航 + IPC 命令清单）和 `.omp/rules/`（工程纪律，实施时必须遵守）
3. 第 4 节（数据模型）与第 7 节（后端改动清单）——这是动手改的核心

---

## 1. 背景与目标

用户此前使用 **Rolan Pro**（Windows 快速启动面板，¥49/年）做桌面图标启动面板。经讨论确认：

- **不做**破解/绕过 Rolan 授权（法律与立场问题），本项目是**独立实现**的功能，不复制 Rolan 的任何代码。
- 选择在开源项目 **ZeroLaunch-rs**（Rust + Tauri 2 + Vue 3，GPL-3.0-only，插件化启动器）上以 fork 形式**新增一个 Rolan 风格的图标面板**。
- 选 fork 而非另写独立程序的原因：ZeroLaunch-rs 已有完整的图标提取与两级缓存、应用启动执行器、配置持久化系统、设置窗口、i18n、窗口失焦隐藏等基础设施，另写等于全部重造。

**目标形态一句话**：主窗口在**空输入**时，搜索框下方显示「tab 分类栏 + 图标网格」，点击图标启动；一旦开始输入，tab 和图标整个隐藏，换回现有候选列表；清空输入切回面板。

## 2. 交互规格（已确认）

```
打开（热键/托盘）→ 面板出现，焦点已在输入框（现有行为，无需改动）
├─ 输入框为空 → 显示：tab 分类栏 + 当前分类的图标网格
└─ 开始输入   → tab 和图标整个隐藏 → 常规候选列表（现有行为）
    清空输入 → 切回面板，且记住上次选中的 tab（会话内记忆即可）

┌──────────────────────────────────────────────────┐
│  🔍  输入…                                        │ ← 现有搜索框，不动
├──────────────────────────────────────────────────┤
│  [常用] [开发] [游戏] [网页]                       │ ← tab 栏
│                                                  │
│    ▣      ▣      ▣      ▣      ▣                 │ ← 图标 48–64px
│   微信   编辑器  终端   浏览器  下载               │    下方单行文字，超长省略
│    ▣      ▣      ▣                               │
│   文件夹  网址   关机                             │
└──────────────────────────────────────────────────┘
```

细节规则：

| 项目 | 决定 |
|---|---|
| 打开即聚焦输入框 | 沿用现有能力（`SearchBar.vue` 自带 autofocus + 唤醒重聚焦），不做改动 |
| 单击图标 | 启动目标，然后隐藏窗口（启动器惯例，沿用现有行为） |
| 右键条目 | 小菜单：编辑 / 移除 / 打开所在文件夹（仅当目标有路径） |
| 空分类 | 显示提示"这里是空的，拖个程序进来或去设置里添加" |
| 面板无任何分类 | 显示引导"去设置 → 图标面板 创建第一个分类" |
| 与现有 home 候选的关系 | **面板启用时面板优先**；home 候选功能（默认关闭）仅在面板关闭时生效 |
| Pin / 常驻 | **v1 不做**。默认失焦即隐藏（与 Rolan 默认一致）；仓库里不存在 pin 能力，要加属于独立需求 |
| 键盘导航（方向键选格子 + Enter） | **v1 不做**（鼠标为主；"打开即打字"已是键盘路径），列入 v1.5 |
| 拖拽排序 | 面板内不做；排序在管理页（v1 用上下移按钮，拖拽排序 v1.5） |
| 主题 | 图标格与文字颜色必须走现有 CSS 变量（见 `.omp/rules/css-variable-contract.md`），自动适配明暗主题 |

## 3. 现状调研结论（2026-10-13 完成，基线 v1.4.1）

### 3.1 直接可复用的现成能力

| 能力 | 位置与说明 |
|---|---|
| **面板挂点** | `src-ui/views/SearchView.vue:22-30` 是主窗口决定"搜索框下方渲染什么"的唯一位置（`v-if="(uiMode==='search' \|\| uiMode==='inline_param') && (!searchStore.isIdle \|\| searchStore.isHomeActive)"`）。新增一个空态分支渲染 `IconPanel` 即可 |
| **"空输入"信号** | `search-store.ts` 的 `isIdle`（query 为空）与 `isHomeActive`（现有 home 功能激活）。面板模式需要自己的开关（见第 4 节 `enabled`），命中条件：`isIdle && panelEnabled` |
| **图标提取 + 两级缓存** | Rust 侧 `PluginHandle::get_icon_or_default(IconRequest) -> Vec<u8>`（`crates/plugin-api/src/host/plugin_handle.rs:84`）；缓存 L1 内存 DashMap + L2 磁盘 `<app_data>/icons/*.webp`，键为 IconRequest 的 blake3；`IconRequest` 支持 `Path / Url / Extension / Data` 四种（`crates/plugin-api/src/services/icon_request.rs:8-19`）；Windows 提取实现 `crates/platform-windows/src/icon.rs` |
| **配置持久化** | `<config_dir>/zerolaunch_config.json`（`src-tauri/src/core/config/store.rs:20-22`，Windows 下为 `%USERPROFILE%\.ZeroLaunch-rs\config`），格式 `{ version, components: { <id>: { enabled, settings } } }`。**新增配置组件只需实现 `Configurable` + `inventory::submit!` 注册**，模板：`src-tauri/src/builtin_plugin/config/icon_override_config.rs`（234 行）与 `candidate_registry_config.rs` |
| **自定义图标机制** | `icon_override_config.rs` 已实现"用户选一个图标文件覆盖某目标图标"的完整流程（含配置 action），可作为管理页"换图标"的参考 |
| **设置窗口 + 自定义页面先例** | 设置窗口在 `src-tauri/src/lib.rs:290-317` 创建（`WebviewWindowBuilder`，1150×650，`drag_and_drop(true)`）；侧边栏静态项列表在 `src-ui/utils/settingsSidebar.ts:46-55`，页面路由是 `src-ui/views/SettingsView.vue` 里的 `v-if` 链（`:28` 的 `plugins-page` 分支就是**非 schema 自定义页**的先例）。加一个"图标面板"管理页 = 侧边栏加一项 + 一个 `v-else-if` |
| **文件拖拽样板** | `src-ui/views/settings/PluginsManagement.vue:249-280`（`getCurrentWebview().onDragDropEvent(...)` + 路径校验），设置窗口直接可用 |
| **窗口高度自适应** | `src-ui/composables/useWindowResize.ts`（ResizeObserver 盯 `.window-frame` 高度 → `appWindow.setSize`）。面板内容变高，窗口自动跟随，**无需接线**；但要给网格设 `max-height + overflow-y:auto`，否则图标多了窗口会长到吞屏 |
| **点击启动（路径类）** | 前端已有 `shell:allow-open` 权限（`src-tauri/capabilities/default.json`）且已在用 `shellOpen()`（`src-ui/bridge/commands.ts:412`） |
| **后端执行器（全类型）** | `src-tauri/src/builtin_plugin/executor/` 下已有 `app_executor`（`launch_app`）、`path_executor`（`shell_open` / 管理员提升 / 打开文件夹）、`url_executor`、`file_executor`、`command_executor`、`builtin_command_executor` |
| **索引候选 + 图标（批量）** | `candidate-registry` 组件的 `search_candidates` action 能返回带 base64 图标的索引候选（`candidate_registry_config.rs:114-155`） |

### 3.2 唯一的真空（本项目的主要后端工作量）

**现有 `bridge_confirm` 只接受"活候选缓存里的 candidate id"（带 generation 校验），无法执行任意存储的条目**（`commands/bridge.rs:378-452` → `session_dispatcher.rs:1269-1329`）。因此需要一个新命令 `panel_launch`，按面板条目直接分派到上述执行器/host API。**这是全方案唯一需要现场验证挂法的点**（预计改动小，但不要假设，先验证）。

### 3.3 明确不存在的东西（别去找）

- **没有 pin / 常驻窗口能力**（唯一的 `set_always_on_top` 调用是设置窗口的抢焦点技巧，`src-tauri/src/window/mod.rs:128-137`）
- **没有任何拖拽排序实现**（全仓库 grep 无结果，要自己写）
- **主窗口没有拖拽处理器**（`dragDropEnabled: true` 开着，但拖文件进来什么都不发生）
- **没有"按路径取图标"的命令**（图标只在 `bridge_query` 结果里内联返回）——需要新的批量命令
- **没有收藏/固定项类存储**（`icon-override-config` 是结构最接近的先例）
- **没有配置迁移机制**（只有 serde `#[serde(default)]` + 校验兜底；新增字段务必带 default）
- `src-ui/components/panel/EmptyState.vue` 是死代码（零引用），可改造复用
- 设置窗口**没有 router**（纯组件状态驱动）；`src-ui/router/index.ts` 里的 `/settings` 路由是废的

### 3.4 工程约束（`.omp/rules/` 节选，实施时必须遵守）

- Vue 文件**禁止手写 `<svg>`**，一律用 `lucide-vue-next` 组件（`.omp/rules/icon-library.md`）
- 配置组件 `apply_settings` 内**禁止 spawn async**，副作用必须放 `on_settings_changed`（`.omp/rules/core-config-component.md`）
- 颜色/间距走 CSS 变量合同（`.omp/rules/css-variable-contract.md`）
- i18n 三个语言文件（`zh-Hans / zh-Hant / en`）**必须同步**，且 Rust 侧读的是同一份文件（构建时从 `src-ui/i18n/locales/` 复制到 `src-tauri/locales/`），只改一边会导致后端取不到文案
- 提交用 Conventional Commits（`CONTRIBUTING.md`）

## 4. 数据模型（已确认）

新增配置组件 **`icon-panel-config`**，落在 `src-tauri/src/builtin_plugin/config/`，照 `icon_override_config.rs` 模板（实现 `Configurable` + `inventory::submit!`）。

持久化形态（`zerolaunch_config.json` 内的该组件 `settings`）：

```jsonc
{
  "enabled": true,               // 空输入时显示面板（关闭则一切照旧）
  "tabs": [
    {
      "id": "t_ab12cd",          // 稳定 id（uuid/slug），重命名不影响引用
      "name": "常用",
      "entries": [
        {
          "id": "e_xy34zz",
          "title": "微信",                                  // 默认从目标推导，可改
          "target": { "kind": "path", "path": "C:\\Program Files\\...\\WeChat.exe" },
          "icon":   { "path": "C:\\Program Files\\...\\WeChat.exe" },   // IconRequest JSON
          "iconOverridePath": null                          // 可选：用户自选图标文件
        }
      ]
    }
  ]
}
```

`target` 是一个**类型标记枚举**，五类，与后端既有执行器一一对应：

| kind | 载荷 | 启动分派 |
|---|---|---|
| `path` | `{ path }` | `shell_open(OpenTarget::File)`；.lnk 由系统解析 |
| `url` | `{ url }` | `shell_open(url)`（也用于 `ms-settings:` 这类协议） |
| `app` | `{ appId }` | `launch_app(app_id)`（索引里的应用，**商店/UWP 应用只能走这里**） |
| `command` | `{ program, args: [], runAsAdmin: bool }` | 命令执行器 / 管理员提升 |
| `builtin` | `{ actionId }` | 内置动作执行器（动作清单**实现时盘点**，见第 10 节风险） |

三个关键设计决策（已确认）：

1. **图标存"请求"不存图片**：条目只记录 `IconRequest`（路径/URL/扩展名），渲染时批量向后端要 data URL——直接命中现成的两级缓存，**零新图标管线**。
2. **"从搜索结果右键添加"走后端命令**：候选 id 只在当前会话内有效（generation 校验），所以不是前端存 id，而是发 `panel_add_from_candidate(candidate_id, generation, tab_id)`，**后端从活的候选缓存取出真实目标**、生成 IconRequest 与默认标题、落入配置。安全且拿到的是真数据。
3. **不需要后端解析的写操作走通用配置通道**：改标题、改排序、删除、创建分类 = 前端改完整个 settings 对象后调现有 `config_apply_settings`；只有"从候选添加 / 从路径添加"这类需要后端解析的才加专用命令。**新命令面尽量小。**

## 5. 面板 UI（已确认）

- 新组件 `src-ui/components/panel/IconPanel.vue` + store `src-ui/stores/panel-store.ts`（Vue 3 setup store，仿 `search-store.ts`）
- 结构：tab 栏（横向，超出可横向滚动）+ 图标网格
- 网格：`grid-template-columns: repeat(auto-fill, minmax(80px, 1fr))`——窗口宽度用户可配（默认 800px，见 `appearance-config.window_width`），**不能写死列数**
- 图标格：图标 48–64px + 下方单行文字（超长省略号）；hover 有反馈（沿用现有 CSS 变量的 hover 色）
- 滚动：网格容器 `max-height` + `overflow-y: auto`，滚动条样式仿 `ResultList.vue:79-88`
- **每个可点击格子必须带 `data-no-drag`**：`WindowFrame.vue:22-32` 会在 mousedown 时拖窗口（只豁免 input/button 等标签和 `[data-no-drag]` 元素），漏了会导致"点图标变成拖窗口"
- 空态引导文案两种（空分类 / 无分类），文案进 i18n 三个文件
- 图标渲染复用 `src-ui/components/common/IconDisplay.vue`（已处理 http/data/相对路径三种来源）

## 6. 管理页（已确认）

位置：设置窗口新增「图标面板」页。

- 侧边栏注册：`src-ui/utils/settingsSidebar.ts:46-55` 加一项（新 type 或复用 `plugins-page` 式自定义 type）
- 页面挂载：`src-ui/views/SettingsView.vue` 的 `v-if` 链加一个 `v-else-if` 分支（照 `:28` 的 `PluginsManagement`）
- 新组件：`src-ui/views/settings/IconPanelManagement.vue`

```
┌─ 设置 · 图标面板 ────────────────────────────────────┐
│  分类                 │  [常用]  的图标                  │
│  ──────────────       │  ┌──────┬──────┬──────┐        │
│  ▸ 常用        ↑↓     │  │ ▣ 微信│ ▣ 终端│ ▣ 下载│  ✎ ✕  │
│    开发        ↑↓ ✕   │  │  程序 │  程序 │ 文件夹│        │
│    游戏        ↑↓ ✕   │  ├──────┼──────┼──────┤        │
│    ＋ 新建分类         │  │ ▣ 关机│                    │
│                       │  │  命令 │                    │
│                       │  └──────┴──────┴──────┘        │
│                       │  ┌─ 拖文件到这里添加 ─┐          │
│                       │  │  或 [浏览文件…] [添加命令…] │  │
│                       │  └───────────────────┘          │
└──────────────────────────────────────────────────────┘
```

功能清单（v1）：
- 分类：新建 / 重命名 / 删除 / 上下移动
- 条目：改名 / 换图标（选图标文件，存 `iconOverridePath`）/ 移除 / 上下移动
- 添加三通道：
  1. **搜索结果右键**（主窗口）：`ResultItem` 右键菜单加「添加到面板 → 选分类」（弹层列出分类）
  2. **拖拽 + 文件选择器**（管理页内）：拖拽抄 `PluginsManagement.vue:249-280` 样板；支持 .exe/.lnk/文件夹/文件
  3. **添加命令**（管理页弹窗）：程序/命令 + 参数 + 是否管理员 + 显示名；内置动作从后端清单选
- **v1 不做**：拖拽排序（用上下移按钮）、分类图标自定义、批量导入导出

## 7. 后端改动清单

| # | 改动 | 落点/模板 | 备注 |
|---|---|---|---|
| 1 | 新配置组件 `icon-panel-config` | `src-tauri/src/builtin_plugin/config/`，模板 `icon_override_config.rs` | 字段全部 `#[serde(default)]`（无迁移机制） |
| 2 | `panel_add_from_candidate(tabId, candidateId, generation)` | 复用 `SessionDispatcher` 的候选缓存读取 + `get_icon_or_default` | 生成 target + IconRequest + 默认标题后落入配置 |
| 3 | `panel_add_path(tabId, path)` | 解析 .lnk 目标、按扩展名推导 IconRequest、标题取文件名词干 | 拖拽/选择器共用 |
| 4 | `panel_launch(entryId, action?)` | **唯一需现场验证的点**。按 target.kind 分派：path/url→`PluginHandle::shell_open`；app→`launch_app`；command→命令执行器；builtin→内置动作。`action` 支持 `open` / `open_folder` / `run_as_admin`（后两者仅 path 类） | 参考 `path_executor.rs:35-60` 的调用方式；不追求复用 `bridge_confirm` 的会话链路 |
| 5 | `panel_get_icons()` | 返回 `{ entryId: dataUrl }`；有 `iconOverridePath` 时优先读该文件转 data URL | 走既有 `ImageUtils::to_data_url` |
| 6 | 编辑类操作（改名/排序/删除） | **不加新命令**，走现有 `config_apply_settings('icon-panel-config', settings)` | 前端持有完整 settings 对象 |

**不改**：插件框架（`plugin_framework/`）、会话调度器内部逻辑、执行器注册表。只调用，不修改。

前端改动清单：

| 文件 | 改动 |
|---|---|
| `src-ui/components/panel/IconPanel.vue` | 新建（tab 栏 + 图标网格） |
| `src-ui/stores/panel-store.ts` | 新建（条目数据、当前 tab、会话内 tab 记忆；仿 `search-store.ts`） |
| `src-ui/bridge/commands.ts` | 加 5 个 wrapper（照 `:102-104` 的 `invokeCommand` 模式，camelCase 参数）；类型进 `contract.ts` |
| `src-ui/views/SearchView.vue` | `:22-30` 处加面板分支（`isIdle && panelEnabled` 优先于 ResultList） |
| `src-ui/components/results/ResultItem.vue` | 右键菜单加「添加到面板 → 选分类」 |
| `src-ui/views/settings/IconPanelManagement.vue` | 新建管理页 |
| `src-ui/utils/settingsSidebar.ts` + `src-ui/views/SettingsView.vue` | 侧边栏项 + 页面分支 |
| `src-ui/i18n/locales/{zh-Hans,zh-Hant,en}.json` | 三个文件同步加文案（面板 + 管理页 + 右键菜单） |

## 8. 分阶段计划与验收

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **0. 环境** | 装 bun（≥1.2.22）、`rustup update stable`（≥1.90.0）、`bun install`、`cargo check --workspace` 跑通、`bun run tauri dev` 起得来 | dev 窗口能打开并能搜索（首次构建最贵，先付掉） |
| **1. 能看能点** | 后端 #1 + #4 + #5；前端 `IconPanel.vue` + store + SearchView 分支；先手写 JSON 数据验证通路 | 手写两条数据 → 面板显示 tab 和图标 → 点图标启动程序；开始输入切回候选列表、清空切回面板 |
| **2. 能添加** | 后端 #2 #3 #6；三种添加方式 + 管理页（增删改、上下移、换图标） | 右键从搜索结果加微信、拖一个文件夹进去、加一条关机命令；重启应用后全都在 |
| **3. 打磨** | 拖拽排序、键盘导航（方向键+Enter）、面板右键菜单完善、内置动作清单补齐、空态文案 | 手感验收（对照 Rolan 的使用习惯） |

每阶段完成后跑 `just ci`（clippy + fmt + test）或等价命令，遵守仓库提交规范。

## 9. 风险与未验证项（诚实清单）

1. **`panel_launch` 的挂法未验证**——现有链路只认候选缓存，新命令要绕过会话直接分派。预期是小改动，但这是全案唯一没读过实现细节的点，**先验证再铺开**。
2. **内置动作清单可能比预期少**——"关机/锁屏/打开设置"大多可以用 `command`（如 `shutdown /s /t 0`）或 `url`（如 `ms-settings:`）表达，`builtin` 子类型实现时盘点后端 `builtin_command_executor` 实际暴露了什么，不够就用 command/url 兜底。
3. **首次构建成本未知**——调研只读代码没编译过。Tauri 首次全量编译在同类项目上通常是十几分钟到几十分钟 + 数 GB 磁盘（上游作者的 `target/` 在自己机器上是 116GB，属异常值，但要有心理预期）。
4. **`config_apply_settings` 从主窗口调用的行为未验证**——目前它只被设置窗口调用过；如果从主窗口调用有权限/时序问题（面板右键"移除"会用到），退路是给编辑类操作也加一个专用命令。
5. **超长分类名/大量图标的性能**——网格是纯前端渲染 + 图标 data URL，几百个条目的场景没测过；v1 按几十个条目设计。

## 10. 环境前置

| 要求 | 状态（2026-10-13） |
|---|---|
| Rust ≥ 1.90.0 | 本机 1.87.0，**需 `rustup update stable`** |
| Bun ≥ 1.2.22 | **未安装**（dev/build 命令写死 `bun run dev/build`，lockfile 为 `bun.lock`，不建议换 pnpm） |
| VS 2022 + MSVC | 已装 ✓ |
| WebView2 Runtime | 已装 ✓ |
| git | ✓，fork 结构：`origin`=HeiShuXianSheng/ZeroLaunch-rs，`upstream`=ghost-him/ZeroLaunch-rs |

**许可证注意**：主程序 GPL-3.0-only。自己使用无限制；若要分发修改后的版本，必须同样以 GPL-3.0 开源。插件 SDK 是 Apache-2.0，不受影响。

## 11. 决策日志（讨论中已否决/推迟的选项，避免重复讨论）

| 选项 | 结论 | 原因 |
|---|---|---|
| tab/图标自动从索引生成 | ❌ 否决 | 用户明确要"手工维护 + 管理界面"（Rolan 的用法） |
| 管理页"从已安装程序列表挑选" | ❌ 未选 | "搜索结果右键添加"已覆盖同样需求，少做一套 UI |
| 面板做成独立常驻窗口 / Pin | ⏸ v1 不做 | 现有主窗口失焦即隐藏已符合 Rolan 默认用法；pin 是独立需求 |
| 拖拽文件到"面板本体"添加 | ❌ 否决 | 面板失焦即隐藏，从资源管理器拖出时鼠标离开窗口面板就藏了，交互天然冲突；拖拽放管理页（且那里有现成样板） |
| 键盘导航（方向键+Enter） | ⏸ v1.5 | 面板以鼠标为主；输入即切搜索已是键盘路径 |
| 拖拽排序 | ⏸ v1.5 | 全仓库无先例，需要从零写；v1 用上下移按钮 |
| 面板内容范围 | ✅ 选最全 | 路径 + 商店应用 + 命令/内置动作，五类 target 全支持 |
| 图标存储 | ✅ 存 IconRequest | 复用现成两级缓存，零新管线 |
| fork vs 另写 | ✅ fork | 图标/启动/配置/设置窗口/i18n 全部现成 |
