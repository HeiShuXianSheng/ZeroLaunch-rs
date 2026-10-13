//! 图标面板 IPC 命令 —— 面板条目的图标批量获取与启动。
//!
//! 面板条目持久化在配置组件 `icon-panel-config` 中，其条目 id 只在该组件内有效，
//! 无法走 `bridge_confirm` 的会话候选链路（那条链路只认活候选缓存里的 candidate id，
//! 且带候选世代校验），故这里按条目 id 取出目标后直接交给会话调度器执行。

use crate::builtin_plugin::config::icon_panel_config::{
    panel_target_to_execution_target, IconPanelSettings, PanelEntry, PanelTarget,
};
use crate::commands::bridge_error::{BridgeError, WithTraceId};
use crate::state::app_state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;
use zerolaunch_plugin_api::common::ImageUtils;

/// 图标面板配置组件 id。
const COMPONENT_ID: &str = "icon-panel-config";

/// 面板条目的启动动作（IPC 词表）。
///
/// 仅 `Open` 对全部目标类型有效；`OpenFolder` 与 `RunAsAdmin` 只适用于路径类目标
/// ——宿主只对路径提供「打开所在文件夹」与「以管理员身份运行」两项能力。
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PanelLaunchAction {
    /// 启动目标本体。
    #[default]
    Open,
    /// 打开目标所在文件夹（仅路径类目标）。
    OpenFolder,
    /// 以管理员身份运行目标（仅路径类目标）。
    RunAsAdmin,
}

impl PanelLaunchAction {
    /// 映射到执行器动作 ID（`ExecutorRegistry` 中以该字符串注册动作）。
    ///
    /// 返回：该动作在执行器注册表中的 ID。
    fn executor_action_id(self) -> &'static str {
        match self {
            PanelLaunchAction::Open => "execute",
            PanelLaunchAction::OpenFolder => "open_folder",
            PanelLaunchAction::RunAsAdmin => "execute_admin",
        }
    }
}

/// 面板图标批量查询的返回载荷。
#[derive(Serialize)]
pub struct PanelIconsResponse {
    /// 条目 id → 图标 data URL；无图标时为空字符串（前端据此渲染占位）。
    #[serde(rename = "icons")]
    pub icons: HashMap<String, String>,
}

/// 批量读取面板条目的图标，返回条目 id 到 data URL 的映射。
///
/// 入参：无（一次返回全部分类的条目，条目规模按几十条设计）。
/// 返回：`PanelIconsResponse`；配置读取失败时报 `BridgeError`。
#[tauri::command]
#[tracing::instrument(skip(state), fields(trace_id))]
pub async fn panel_get_icons(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PanelIconsResponse, BridgeError> {
    let trace_id = crate::utils::trace_id::generate_trace_id();
    tracing::Span::current().record("trace_id", trace_id.as_str());

    let settings = load_panel_settings(&state).with_trace_id(&trace_id)?;
    let core_handle = state.get_core_handle();

    let mut icons = HashMap::new();
    for entry in settings.tabs.iter().flat_map(|tab| tab.entries.iter()) {
        if entry.id.is_empty() {
            continue;
        }
        let data = match entry.icon_override_path.as_deref() {
            // 条目自选图标文件优先：直接读该文件转 data URL。覆盖文件是该条目的
            // 私有属性，不进图标缓存，与全局「图标覆盖」组件的语义互不影响。
            Some(path) if !path.is_empty() => {
                read_icon_file(path).await.with_trace_id(&trace_id)?
            }
            _ => core_handle.get_icon_or_default(entry.icon.clone()).await,
        };
        icons.insert(entry.id.clone(), ImageUtils::to_data_url(&data));
    }
    Ok(PanelIconsResponse { icons })
}

/// 启动一个面板条目。
///
/// 入参：entry_id - 条目 id；action - 启动动作，缺省为 `Open`。
/// 返回：无数据；条目不存在、动作与目标类型不匹配或启动失败时报 `BridgeError`。
#[tauri::command]
#[tracing::instrument(skip(state), fields(trace_id))]
pub async fn panel_launch(
    state: tauri::State<'_, Arc<AppState>>,
    entry_id: String,
    action: Option<PanelLaunchAction>,
) -> Result<(), BridgeError> {
    let trace_id = crate::utils::trace_id::generate_trace_id();
    tracing::Span::current().record("trace_id", trace_id.as_str());

    let action = action.unwrap_or_default();
    let settings = load_panel_settings(&state).with_trace_id(&trace_id)?;
    let entry = find_entry(&settings, &entry_id)
        .ok_or_else(|| BridgeError::not_found(&format!("面板条目 {}", entry_id)))
        .with_trace_id(&trace_id)?;

    validate_action(&entry.target, action).with_trace_id(&trace_id)?;
    validate_target(&entry.target).with_trace_id(&trace_id)?;

    let display_name = if entry.title.is_empty() {
        entry_id.as_str()
    } else {
        entry.title.as_str()
    };
    debug!(
        "面板条目启动: entry='{}', action={:?}",
        display_name, action
    );

    let target = panel_target_to_execution_target(&entry.target);
    state
        .get_session_dispatcher()
        .execute_target(target, display_name, action.executor_action_id(), &[])
        .await
        .with_trace_id(&trace_id)
}

/// 读取面板配置组件的当前设置。
///
/// 入参：state - 应用状态。
/// 返回：解析后的面板设置；组件未注册或设置形状不符时报 `BridgeError`。
fn load_panel_settings(state: &AppState) -> Result<IconPanelSettings, BridgeError> {
    let raw = state
        .get_config_manager()
        .get_settings(COMPONENT_ID)
        .ok_or_else(|| BridgeError::not_found(COMPONENT_ID))?;
    serde_json::from_value(raw)
        .map_err(|e| BridgeError::internal(format!("解析 {} 设置失败: {}", COMPONENT_ID, e)))
}

/// 在整份面板配置中按 id 查找条目。
///
/// 入参：settings - 面板设置；entry_id - 条目 id。
/// 返回：命中的条目引用；未命中返回 `None`。
fn find_entry<'a>(settings: &'a IconPanelSettings, entry_id: &str) -> Option<&'a PanelEntry> {
    settings
        .tabs
        .iter()
        .flat_map(|tab| tab.entries.iter())
        .find(|entry| entry.id == entry_id)
}

/// 校验启动动作与目标类型是否匹配。
///
/// 入参：target - 条目目标；action - 待执行动作。
/// 返回：动作可用时 `Ok(())`；对非路径目标使用 `OpenFolder` / `RunAsAdmin` 时报校验错误。
fn validate_action(target: &PanelTarget, action: PanelLaunchAction) -> Result<(), BridgeError> {
    if action == PanelLaunchAction::Open || matches!(target, PanelTarget::Path { .. }) {
        return Ok(());
    }
    Err(BridgeError::validation_failed(format!(
        "动作 {:?} 仅适用于路径类目标",
        action
    )))
}

/// 校验条目自身声明的约束。
///
/// 入参：target - 条目目标。
/// 返回：约束满足时 `Ok(())`；声明了无法兑现的能力时报校验错误
/// （命令类目标声明管理员运行时，平台提升接口只接受程序路径、不接受参数，
/// 静默按非提升运行会让用户误以为已获得特权，故明确拒绝）。
fn validate_target(target: &PanelTarget) -> Result<(), BridgeError> {
    if let PanelTarget::Command {
        run_as_admin: true, ..
    } = target
    {
        return Err(BridgeError::validation_failed(
            "命令类目标不支持以管理员身份运行（平台提升接口不接受参数），请取消该选项或改用路径类目标",
        ));
    }
    Ok(())
}

/// 读取图标覆盖文件的内容。
///
/// 入参：path - 图标文件路径（用户自选）。
/// 返回：文件字节；读取失败时报 `BridgeError`。
async fn read_icon_file(path: &str) -> Result<Vec<u8>, BridgeError> {
    tokio::fs::read(path)
        .await
        .map_err(|e| BridgeError::internal(format!("读取图标文件失败: {} ({})", e, path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三个 IPC 动作各自映射到执行器注册表中的动作 ID。
    #[test]
    fn actions_map_to_executor_ids() {
        assert_eq!(PanelLaunchAction::Open.executor_action_id(), "execute");
        assert_eq!(
            PanelLaunchAction::OpenFolder.executor_action_id(),
            "open_folder"
        );
        assert_eq!(
            PanelLaunchAction::RunAsAdmin.executor_action_id(),
            "execute_admin"
        );
        assert_eq!(PanelLaunchAction::default(), PanelLaunchAction::Open);
    }

    /// 动作缺省值为 open（前端可省略 action 字段）。
    #[test]
    fn action_deserializes_from_snake_case() {
        assert_eq!(
            serde_json::from_str::<PanelLaunchAction>("\"open\"").unwrap(),
            PanelLaunchAction::Open
        );
        assert_eq!(
            serde_json::from_str::<PanelLaunchAction>("\"open_folder\"").unwrap(),
            PanelLaunchAction::OpenFolder
        );
        assert_eq!(
            serde_json::from_str::<PanelLaunchAction>("\"run_as_admin\"").unwrap(),
            PanelLaunchAction::RunAsAdmin
        );
    }

    /// 文件夹与提权动作只对路径类目标放行。
    #[test]
    fn folder_and_admin_actions_are_path_only() {
        let path = PanelTarget::Path {
            path: r"C:\a.exe".into(),
        };
        assert!(validate_action(&path, PanelLaunchAction::OpenFolder).is_ok());
        assert!(validate_action(&path, PanelLaunchAction::RunAsAdmin).is_ok());

        let url = PanelTarget::Url {
            url: "https://e.com".into(),
        };
        assert!(validate_action(&url, PanelLaunchAction::Open).is_ok());
        assert!(validate_action(&url, PanelLaunchAction::OpenFolder).is_err());
        assert!(validate_action(&url, PanelLaunchAction::RunAsAdmin).is_err());

        let app = PanelTarget::App {
            app_id: "MS.Calc".into(),
        };
        assert!(validate_action(&app, PanelLaunchAction::RunAsAdmin).is_err());
    }

    /// 错误路径必须带 trace_id。
    ///
    /// `BridgeError` 的 `trace_id` 为空时，序列化会触发 `bridge_error.rs` 的
    /// `debug_assert!`（debug 构建直接 panic，命令永不返回）——真机验证中曾因此
    /// 让 `panel_launch` 卡死。此处固定命令中每条错误路径的返回惯用法。
    #[test]
    fn error_paths_carry_trace_id() {
        let trace_id = "trace-under-test";
        let err = validate_action(
            &PanelTarget::Url {
                url: "https://example.com".into(),
            },
            PanelLaunchAction::OpenFolder,
        )
        .with_trace_id(trace_id)
        .expect_err("非路径目标不允许 open_folder");
        assert_eq!(err.trace_id, trace_id);

        let json = serde_json::to_string(&err).expect("BridgeError 应可序列化");
        assert!(json.contains(trace_id), "{json}");
    }

    /// 命令类目标声明管理员运行时被明确拒绝，不静默降级。
    #[test]
    fn command_target_rejects_run_as_admin() {
        let elevated = PanelTarget::Command {
            program: "svc.exe".into(),
            args: vec![],
            run_as_admin: true,
        };
        assert!(validate_target(&elevated).is_err());

        let plain = PanelTarget::Command {
            program: "svc.exe".into(),
            args: vec![],
            run_as_admin: false,
        };
        assert!(validate_target(&plain).is_ok());
    }

    /// 跨分类按 id 查找条目。
    #[test]
    fn find_entry_searches_all_tabs() {
        let settings: IconPanelSettings = serde_json::from_str(
            r#"{"tabs":[
                {"id":"t1","name":"常用","entries":[{"id":"e1","title":"A"}]},
                {"id":"t2","name":"开发","entries":[{"id":"e2","title":"B"}]}
            ]}"#,
        )
        .unwrap();
        assert_eq!(find_entry(&settings, "e1").unwrap().title, "A");
        assert_eq!(find_entry(&settings, "e2").unwrap().title, "B");
        assert!(find_entry(&settings, "nope").is_none());
    }
}
