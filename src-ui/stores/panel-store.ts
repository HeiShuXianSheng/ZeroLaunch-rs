import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { configGetSettings, panelGetIcons, panelLaunch } from '../bridge/commands'
import type { IconPanelSettings, PanelEntry, PanelLaunchAction, PanelTab } from '../bridge/contract'

/** 图标面板配置组件 id（配置读取与配置变更事件的判据）。 */
const COMPONENT_ID = 'icon-panel-config'

/**
 * 图标面板 store —— 空查询时显示的「分类 tab + 图标网格」的数据与交互状态。
 *
 * 数据权威在后端配置组件 `icon-panel-config`：本 store 只做读取缓存与交互状态
 * （当前分类、图标映射），不产生业务裁决。
 */
export const usePanelStore = defineStore('panel', () => {
  /** 面板开关（配置 `enabled`）：关闭时一切沿用原有搜索行为。 */
  const enabled = ref(false)
  /** 分类列表，顺序即 tab 顺序。 */
  const tabs = ref<PanelTab[]>([])
  /** 当前分类 id；会话内记忆（窗口隐藏 / 重开仍停在该分类），不写入配置。 */
  const activeTabId = ref<string | null>(null)
  /** 条目 id → 图标 data URL。 */
  const icons = ref<Record<string, string>>({})

  /** 当前分类：记录的 id 失效（分类被删）时回落到首个分类；无分类时为 null。 */
  const activeTab = computed<PanelTab | null>(() => {
    if (tabs.value.length === 0) return null
    return tabs.value.find((tab) => tab.id === activeTabId.value) ?? tabs.value[0]
  })

  /** 是否完全无分类（渲染「去设置创建第一个分类」引导）。 */
  const hasTabs = computed(() => tabs.value.length > 0)

  /**
   * 从后端重载面板配置与图标。
   * 由视图挂载与配置变更事件调用；读取失败按「面板关闭」处理，不影响搜索主流程。
   */
  async function loadFromBackend() {
    try {
      const raw = (await configGetSettings(COMPONENT_ID)) as IconPanelSettings | null
      enabled.value = raw?.enabled === true
      tabs.value = Array.isArray(raw?.tabs) ? raw.tabs : []
      // 记录的分类已不存在（被删除）时清空，由 activeTab 回落到首个分类
      if (activeTabId.value && !tabs.value.some((tab) => tab.id === activeTabId.value)) {
        activeTabId.value = null
      }
      await loadIcons()
    } catch (e) {
      console.warn('[panel-store] 读取图标面板配置失败:', e)
      enabled.value = false
      tabs.value = []
      icons.value = {}
    }
  }

  /** 批量读取条目图标（后端图标两级缓存），整体替换映射。 */
  async function loadIcons() {
    try {
      const resp = await panelGetIcons()
      icons.value = resp.icons
    } catch (e) {
      console.warn('[panel-store] 读取面板图标失败:', e)
      icons.value = {}
    }
  }

  /** 切换当前分类（会话内记忆）。 */
  function setActiveTab(tabId: string) {
    activeTabId.value = tabId
  }

  /**
   * 启动条目。仅委托后端执行；成功与否都向上抛出，由调用方决定窗口隐藏时机
   * （成功后再隐藏，失败时窗口保持可见以便全局错误处理器给出提示）。
   */
  function launch(entry: PanelEntry, action?: PanelLaunchAction): Promise<void> {
    return panelLaunch(entry.id, action)
  }

  /** 取条目的图标 data URL；无图标时返回空串（组件渲染占位）。 */
  function iconOf(entryId: string): string {
    return icons.value[entryId] ?? ''
  }

  return {
    enabled, tabs, activeTabId, icons,
    activeTab, hasTabs,
    loadFromBackend, loadIcons, setActiveTab, launch, iconOf,
  }
})

/** 图标面板 store 实例类型：在所有者模块命名，消费者按名引用。 */
export type PanelStore = ReturnType<typeof usePanelStore>
