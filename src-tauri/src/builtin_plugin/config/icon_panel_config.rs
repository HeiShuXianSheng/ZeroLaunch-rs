//! 图标面板配置组件 —— 持久化「分类 + 图标条目」的面板数据。
//!
//! 面板本身是前端组件（空输入时显示 tab 分类栏 + 图标网格），本组件只负责
//! 数据的声明、持久化与读写；启动条目由 `commands/panel.rs` 的命令消费。
//! 管理页（`IconPanelManagement.vue`）为自定义页面，直接读写整份 settings，
//! 不经 schema 驱动的表单，故本组件不声明字段 schema（见 `setting_schema`）。

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zerolaunch_plugin_api::config::{ComponentCore, ComponentType, ConfigError, Configurable};
use zerolaunch_plugin_api::plugin::ExecutionTarget;
use zerolaunch_plugin_api::services::IconRequest;

// ============================================================================
// 配置数据结构
// ============================================================================

/// 面板条目的启动目标 —— 五类，与后端既有执行器一一对应。
///
/// 内部标签 `kind`（小写），仅本组件持久化使用；启动时经 `to_execution_target`
/// 转换为 `ExecutionTarget` 交给宿主 API 分发。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelTarget {
    /// 文件路径目标（.exe / .lnk / 任意文件），经系统 shell 打开，.lnk 由系统解析。
    Path {
        /// 目标路径（绝对路径）。
        #[serde(rename = "path", default)]
        path: String,
    },
    /// URL 或协议目标（含 `ms-settings:` 这类自定义协议），经系统 shell 打开。
    Url {
        /// 完整 URL 或协议字符串。
        #[serde(rename = "url", default)]
        url: String,
    },
    /// 索引中的应用目标（商店 / UWP 应用只能走这里），经 `launch_app` 启动。
    App {
        /// 应用标识（应用枚举器产出的 AppId）。
        #[serde(rename = "app_id", default)]
        app_id: String,
    },
    /// 命令行目标，程序与参数由面板侧拼成单条命令字符串后交给 shell 执行。
    Command {
        /// 程序或命令本体。
        #[serde(rename = "program", default)]
        program: String,
        /// 附加参数，按顺序拼接在程序之后。
        #[serde(rename = "args", default)]
        args: Vec<String>,
        /// 是否以管理员身份运行。
        ///
        /// 平台提升接口 `shell_execute_elevation` 只接受程序路径、不支持参数，
        /// 故命令类目标无法被提升；该值为 `true` 时启动命令会明确报错而非
        /// 静默降级为非提升运行（静默降级会让用户误以为已获得特权）。
        /// 管理页对命令类目标不提供该选项，为路径类目标的 `run_as_admin` 动作保留。
        #[serde(rename = "run_as_admin", default)]
        run_as_admin: bool,
    },
    /// 宿主内置动作目标，经 app_command 通道执行。
    Builtin {
        /// 内置动作标识（如 `ShowSettings`）。
        #[serde(rename = "action_id", default)]
        action_id: String,
    },
}

/// 面板条目 —— 图标网格中的一个可点击格子。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelEntry {
    /// 稳定 id（重命名 / 改标题不影响引用）。
    #[serde(rename = "id", default)]
    pub id: String,
    /// 显示标题；默认由目标推导，用户可改。
    #[serde(rename = "title", default)]
    pub title: String,
    /// 启动目标。
    #[serde(rename = "target", default)]
    pub target: PanelTarget,
    /// 图标请求（存请求不存图片，渲染时经两级缓存取 data URL）。
    #[serde(rename = "icon", default = "default_icon_request")]
    pub icon: IconRequest,
    /// 用户自选的图标文件路径；`None` 表示不覆盖，用 `icon` 推导的图标。
    #[serde(rename = "icon_override_path", default)]
    pub icon_override_path: Option<String>,
}

/// 面板分类 —— 一个 tab 及其图标网格内容。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PanelTab {
    /// 稳定 id（重命名不影响条目引用）。
    #[serde(rename = "id", default)]
    pub id: String,
    /// 分类显示名（tab 文案）。
    #[serde(rename = "name", default)]
    pub name: String,
    /// 分类下的条目，顺序即网格渲染顺序。
    #[serde(rename = "entries", default)]
    pub entries: Vec<PanelEntry>,
}

/// 图标面板配置的根结构（持久化于 `zerolaunch_config.json` 该组件的 `settings` 段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconPanelSettings {
    /// 空查询时是否显示图标面板；[`default_panel_enabled`] 为业务默认值。
    #[serde(rename = "enabled", default = "default_panel_enabled")]
    pub enabled: bool,
    /// 分类列表，顺序即面板 tab 顺序。
    #[serde(rename = "tabs", default)]
    pub tabs: Vec<PanelTab>,
}

/// 面板开关的默认值：默认开启。
///
/// 同时作为 serde 默认值（老配置缺该字段时）与 `Default::default()` 的来源，
/// 保证「缺字段」「空配置」「首次运行」三条路径得到同一结果。
fn default_panel_enabled() -> bool {
    true
}

impl Default for IconPanelSettings {
    fn default() -> Self {
        Self {
            enabled: default_panel_enabled(),
            tabs: Vec::new(),
        }
    }
}

/// `IconRequest` 的默认值：空路径请求。
///
/// 仅用于条目缺失 `icon` 字段时的兜底；空请求经 `IconRequest::is_empty()`
/// 判定后由图标服务回落默认图标，不会产生无效提取。
fn default_icon_request() -> IconRequest {
    IconRequest::Path(String::new())
}

/// `PanelTarget` 的默认值：空路径目标。
///
/// 仅用于条目缺失 `target` 字段时的兜底，保证反序列化不整体失败。
impl Default for PanelTarget {
    fn default() -> Self {
        PanelTarget::Path {
            path: String::new(),
        }
    }
}

// ============================================================================
// 转换函数（存储类型 → 运行时类型）
// ============================================================================

/// 将面板目标转换为宿主执行目标。
///
/// 入参：面板条目的 `target`。
/// 返回：可直接交给宿主 API 分发的 `ExecutionTarget`。
/// 说明：`Command` 变体在此把程序与参数拼成一条命令行（宿主命令执行器只接受
/// 单条命令字符串）；各变体与既有执行器一一对应。
pub(crate) fn panel_target_to_execution_target(target: &PanelTarget) -> ExecutionTarget {
    match target {
        PanelTarget::Path { path } => ExecutionTarget::Path(path.clone()),
        PanelTarget::Url { url } => ExecutionTarget::Url(url.clone()),
        PanelTarget::App { app_id } => ExecutionTarget::App(app_id.clone()),
        PanelTarget::Command { program, args, .. } => {
            ExecutionTarget::Command(build_command_line(program, args))
        }
        PanelTarget::Builtin { action_id } => ExecutionTarget::BuiltinCommand(action_id.clone()),
    }
}

/// cmd 元字符：出现在段内时必须用双引号包裹，否则会被 `cmd /C` 当作控制符解释。
const CMD_METACHARS: [char; 8] = ['"', '&', '|', '<', '>', '^', '(', ')'];

/// 将程序与参数拼成一条 cmd 可执行的命令行。
///
/// 入参：program - 程序或命令本体；args - 附加参数，按给定顺序拼接。
/// 返回：以单个空格分隔的命令行，含空白或 cmd 元字符的段用双引号包裹。
pub(crate) fn build_command_line(program: &str, args: &[String]) -> String {
    let mut line = quote_command_segment(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quote_command_segment(arg));
    }
    line
}

/// 按 cmd 规则包裹单个命令行段。
///
/// 入参：segment - 单个程序名或参数。
/// 返回：含空白、为空、或含 `CMD_METACHARS` 的段用双引号包裹并把段内双引号转义为 `\"`；
///       其余原样返回。
fn quote_command_segment(segment: &str) -> String {
    let needs_quoting = segment.is_empty()
        || segment
            .chars()
            .any(|c| c.is_whitespace() || CMD_METACHARS.contains(&c));
    if !needs_quoting {
        return segment.to_string();
    }
    format!("\"{}\"", segment.replace('"', "\\\""))
}

// ============================================================================
// IconPanelConfig — 纯配置组件（ConfigEntry）
// ============================================================================

/// 图标面板配置组件：持有面板分类与条目数据，供管理页读写、供面板命令读取。
pub struct IconPanelConfig {
    core: ComponentCore,
    settings: RwLock<IconPanelSettings>,
}

impl Default for IconPanelConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl IconPanelConfig {
    pub fn new() -> Self {
        Self {
            core: ComponentCore::new(
                "icon-panel-config".to_string(),
                t_key!("icon-panel-config", "name").to_string(),
                t_key!("icon-panel-config", "description").to_string(),
                ComponentType::Core,
                10,
            ),
            settings: RwLock::new(IconPanelSettings::default()),
        }
    }
}

#[async_trait]
impl Configurable for IconPanelConfig {
    fn core(&self) -> &ComponentCore {
        &self.core
    }

    /// 返回空 schema：面板数据是「分类 → 条目 → 目标（带标签联合）」的深层结构，
    /// 超出 schema 支持的 4 层嵌套上限，且目标各变体字段随 `kind` 变化，
    /// 无法用单一 schema 节点表达；管理页为自定义页面，不消费 schema。
    /// 校验由 `validate_settings` 覆写承担。
    fn setting_schema(&self) -> Vec<zerolaunch_plugin_api::config::SettingDefinition> {
        Vec::new()
    }

    /// 覆写默认校验：整棵面板数据结构不在 schema 中表达（见 `setting_schema`）。
    /// 结构合法性由 `PanelTarget`/`PanelEntry` 的强类型反序列化承担——形状不符的
    /// 载荷解析后回落默认值，不会产生半合法状态。
    async fn validate_settings(&self, _settings: &serde_json::Value) -> Result<(), ConfigError> {
        Ok(())
    }

    fn get_settings(&self) -> serde_json::Value {
        serde_json::to_value(self.settings.read().clone()).unwrap_or_default()
    }

    async fn apply_settings(&self, settings: serde_json::Value) -> Result<(), ConfigError> {
        let parsed: IconPanelSettings = serde_json::from_value(settings).unwrap_or_default();
        *self.settings.write() = parsed;
        Ok(())
    }

    fn default_enabled(&self) -> bool {
        true
    }
}

// ============================================================================
// 注册到 inventory（作为 ConfigEntry）
// ============================================================================

use crate::plugin_framework::builtin_registry::{ConfigEntry, InventoryContext};

fn build_icon_panel(_ctx: &InventoryContext) -> Arc<dyn Configurable> {
    Arc::new(IconPanelConfig::new())
}

inventory::submit! {
    ConfigEntry {
        component_id: "icon-panel-config",
        priority: 10,
        factory: build_icon_panel,
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认配置应开启面板且无分类（首次运行 / 空配置路径）。
    #[test]
    fn default_settings_enable_panel() {
        let s = IconPanelSettings::default();
        assert!(s.enabled, "面板默认应开启");
        assert!(s.tabs.is_empty(), "默认不应有分类");
    }

    /// 老配置缺字段时逐字段回落默认值，不得整体反序列化失败。
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: IconPanelSettings = serde_json::from_str("{}").expect("空对象应可反序列化");
        assert!(s.enabled);
        assert!(s.tabs.is_empty());

        // 缺 enabled，只有 tabs
        let s: IconPanelSettings =
            serde_json::from_str(r#"{"tabs":[{"id":"t1","name":"常用","entries":[]}]}"#)
                .expect("缺 enabled 应可反序列化");
        assert!(s.enabled, "缺 enabled 时应回落为 true");
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].entries.len(), 0);
    }

    /// 条目缺 target / icon / icon_override_path 时仍可反序列化。
    #[test]
    fn entry_missing_optional_fields() {
        let s: IconPanelSettings = serde_json::from_str(
            r#"{"tabs":[{"id":"t1","name":"常用","entries":[{"id":"e1","title":"微信"}]}]}"#,
        )
        .expect("条目缺字段应可反序列化");
        let e = &s.tabs[0].entries[0];
        assert_eq!(e.title, "微信");
        assert_eq!(
            e.target,
            PanelTarget::Path {
                path: String::new()
            }
        );
        assert!(e.icon_override_path.is_none());
    }

    /// 五类目标各自按 `kind` 标签序列化 / 反序列化。
    #[test]
    fn panel_target_kind_tags_round_trip() {
        let cases: Vec<(PanelTarget, &str)> = vec![
            (
                PanelTarget::Path {
                    path: r"C:\a\b.exe".into(),
                },
                "path",
            ),
            (
                PanelTarget::Url {
                    url: "https://example.com".into(),
                },
                "url",
            ),
            (
                PanelTarget::App {
                    app_id: "Microsoft.WindowsCalculator".into(),
                },
                "app",
            ),
            (
                PanelTarget::Command {
                    program: "shutdown".into(),
                    args: vec!["/s".into(), "/t".into(), "0".into()],
                    run_as_admin: false,
                },
                "command",
            ),
            (
                PanelTarget::Builtin {
                    action_id: "ShowSettings".into(),
                },
                "builtin",
            ),
        ];
        for (target, kind) in cases {
            let v = serde_json::to_value(&target).expect("序列化失败");
            assert_eq!(
                v.get("kind").and_then(|k| k.as_str()),
                Some(kind),
                "kind 标签不符: {v}"
            );
            let back: PanelTarget = serde_json::from_value(v.clone()).expect("反序列化失败");
            assert_eq!(back, target, "往返不一致: {v}");
        }
    }

    /// 五类目标各自映射到对应的执行目标；command 变体拼成命令行。
    #[test]
    fn target_maps_to_execution_target() {
        assert_eq!(
            panel_target_to_execution_target(&PanelTarget::Path {
                path: r"C:\a\b.exe".into()
            }),
            ExecutionTarget::Path(r"C:\a\b.exe".into())
        );
        assert_eq!(
            panel_target_to_execution_target(&PanelTarget::Url {
                url: "https://e.com".into()
            }),
            ExecutionTarget::Url("https://e.com".into())
        );
        assert_eq!(
            panel_target_to_execution_target(&PanelTarget::App {
                app_id: "MS.Calc".into()
            }),
            ExecutionTarget::App("MS.Calc".into())
        );
        assert_eq!(
            panel_target_to_execution_target(&PanelTarget::Builtin {
                action_id: "ShowSettings".into()
            }),
            ExecutionTarget::BuiltinCommand("ShowSettings".into())
        );
        assert_eq!(
            panel_target_to_execution_target(&PanelTarget::Command {
                program: "shutdown".into(),
                args: vec!["/s".into(), "/t".into(), "0".into()],
                run_as_admin: false,
            }),
            ExecutionTarget::Command("shutdown /s /t 0".into())
        );
    }

    /// 无空白与元字符的段不加引号；含空白或元字符的段加双引号包裹。
    #[test]
    fn command_line_quotes_only_when_needed() {
        assert_eq!(build_command_line("notepad", &[]), "notepad");
        assert_eq!(
            build_command_line("notepad", &["C:\\a b\\f.txt".into()]),
            r#"notepad "C:\a b\f.txt""#
        );
        assert_eq!(
            build_command_line(r"C:\Program Files\App\app.exe", &["--flag".into()]),
            r#""C:\Program Files\App\app.exe" --flag"#
        );
        // cmd 元字符需要引号保护，否则会被 cmd /C 解释
        assert_eq!(
            build_command_line("cmd", &["echo".into(), "a&b".into()]),
            r#"cmd echo "a&b""#
        );
        // 段内双引号转义
        assert_eq!(
            build_command_line("f", &[r#"say "hi""#.into()]),
            r#"f "say \"hi\"""#
        );
    }

    /// 完整配置（文档 §4 形状，snake_case 键）往返一致。
    #[test]
    fn full_settings_round_trip() {
        let raw = r#"{
            "enabled": true,
            "tabs": [{
                "id": "t_ab12cd",
                "name": "常用",
                "entries": [{
                    "id": "e_xy34zz",
                    "title": "微信",
                    "target": { "kind": "path", "path": "C:\\Program Files\\WeChat.exe" },
                    "icon": { "type": "path", "value": "C:\\Program Files\\WeChat.exe" },
                    "icon_override_path": null
                }]
            }]
        }"#;
        let s: IconPanelSettings = serde_json::from_str(raw).expect("应可反序列化");
        assert!(s.enabled);
        assert_eq!(s.tabs.len(), 1);
        let e = &s.tabs[0].entries[0];
        assert_eq!(e.title, "微信");
        assert_eq!(
            e.target,
            PanelTarget::Path {
                path: r"C:\Program Files\WeChat.exe".into()
            }
        );
        assert_eq!(
            e.icon,
            IconRequest::Path(r"C:\Program Files\WeChat.exe".into())
        );
        assert!(e.icon_override_path.is_none());

        // 序列化后必须仍带 enabled / tabs / icon_override_path（不得 skip 空值）
        let back = serde_json::to_value(&s).expect("序列化失败");
        assert!(back.get("enabled").is_some());
        let entry = &back["tabs"][0]["entries"][0];
        assert!(
            entry.get("icon_override_path").is_some(),
            "icon_override_path 应以 null 输出而非被跳过: {entry}"
        );
    }
}
