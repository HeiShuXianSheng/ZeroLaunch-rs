<template>
  <!-- data-no-drag：WindowFrame 在 mousedown 时按 closest('[data-no-drag]') 豁免拖窗，
       挂在根节点即覆盖整块面板（与其下所有格子），否则点图标会变成拖窗口。 -->
  <div class="icon-panel" data-no-drag>
    <!-- 分类 tab 栏：超宽横向滚动。
         格子上的 @mousedown.prevent 阻止默认聚焦：点 tab / 图标后焦点留在搜索框，
         用户可以接着打字（否则焦点落到 body，键盘输入无效）。
         只加在 tab 与格子上，不加在滚动容器上——那会破坏滚动条拖拽。 -->
    <div v-if="panelStore.hasTabs" class="tab-bar">
      <div
        v-for="tab in panelStore.tabs"
        :key="tab.id"
        class="tab"
        :class="{ 'tab-active': activeTabId === tab.id }"
        @mousedown.prevent
        @click="panelStore.setActiveTab(tab.id)"
      >
        {{ tab.name }}
      </div>
    </div>

    <!-- 无任何分类：引导去设置创建 -->
    <div v-if="!panelStore.hasTabs" class="empty-state">
      <n-icon :size="28" color="var(--text-secondary)">
        <LayoutGrid />
      </n-icon>
      <n-text depth="2">{{ $t('iconPanel.noTabs') }}</n-text>
    </div>

    <!-- 空分类：提示拖入或去设置添加 -->
    <div v-else-if="entries.length === 0" class="empty-state">
      <n-icon :size="28" color="var(--text-secondary)">
        <FolderOpen />
      </n-icon>
      <n-text depth="2">{{ $t('iconPanel.emptyTab') }}</n-text>
    </div>

    <!-- 图标网格 -->
    <div v-else class="grid-scroll">
      <div class="grid">
        <div
          v-for="entry in entries"
          :key="entry.id"
          class="grid-cell"
          :title="entry.title"
          @mousedown.prevent
          @click="onLaunch(entry)"
        >
          <IconDisplay :src="panelStore.iconOf(entry.id)" :size="56" />
          <div class="cell-title">{{ entry.title }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NIcon, NText } from 'naive-ui'
import { FolderOpen, LayoutGrid } from 'lucide-vue-next'
import IconDisplay from '../common/IconDisplay.vue'
import { usePanelStore } from '../../stores/panel-store'
import { useSearchStore } from '../../stores/search-store'
import type { PanelEntry } from '../../bridge/contract'

const panelStore = usePanelStore()
const searchStore = useSearchStore()

/** 当前分类的条目；无分类时为空数组。 */
const entries = computed<PanelEntry[]>(() => panelStore.activeTab?.entries ?? [])

/** 当前分类 id（用于 tab 高亮）。 */
const activeTabId = computed(() => panelStore.activeTab?.id ?? null)

/**
 * 单击条目：启动目标，成功后再隐藏窗口。
 * 失败时不隐藏——窗口保持可见，由全局错误处理器提示，避免「窗口已隐藏、错误无处显示」。
 */
async function onLaunch(entry: PanelEntry) {
  try {
    await panelStore.launch(entry)
  } catch {
    return
  }
  searchStore.hideWindow()
}
</script>

<style scoped>
.icon-panel {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  padding: 0 12px 8px;
}

/* ── 分类 tab 栏 ── */
.tab-bar {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
  overflow-x: auto;
  padding-bottom: 6px;
}
/* 隐藏横向滚动条，保持与结果列表一致的素净观感 */
.tab-bar::-webkit-scrollbar {
  height: 0;
}
.tab {
  flex-shrink: 0;
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-md);
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
  transition: background-color 0.12s, color 0.12s;
}
.tab:hover {
  background: var(--hover-color);
  color: var(--text-primary);
}
.tab-active {
  background: var(--hover-color);
  color: var(--accent-color);
}

/* ── 图标网格 ── */
.grid-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* 上限复用「最多可见结果数」这一既有偏好，避免图标多了把窗口撑到吞屏 */
  max-height: calc(var(--result-item-pitch) * var(--max-visible-results));
  margin: 2px 0;
}
.grid-scroll::-webkit-scrollbar {
  width: 4px;
}
.grid-scroll::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 4px;
}
html.dark .grid-scroll::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.1);
}

.grid {
  display: grid;
  /* 列数随窗口宽度自适应；窗口宽度由 appearance-config 决定，不能写死列数 */
  grid-template-columns: repeat(auto-fill, minmax(80px, 1fr));
  gap: 6px;
}
.grid-cell {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 4px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background-color 0.12s;
}
.grid-cell:hover {
  background: var(--hover-color);
}
.cell-title {
  width: 100%;
  /* 单行文字，超长省略 */
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: center;
  font-size: var(--font-size-sm);
  color: var(--text-primary);
}

/* ── 空态 ── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  flex: 1;
  min-height: var(--min-content-height);
  padding: 32px;
  text-align: center;
  font-size: var(--font-size-base);
}
</style>
