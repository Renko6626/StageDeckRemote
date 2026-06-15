<script setup lang="ts">
// 快捷消息预制编辑(导播)+ 提示音开关。
import { ref, onMounted } from "vue";

import { api, type PresetMessage } from "../utils/api";

const presets = ref<PresetMessage[]>([]);
const editing = ref(false);
const drafts = ref<string[]>([]);
const chimeOn = ref(localStorage.getItem("sd.dir.toastChime") !== "off");
const error = ref<string | null>(null);

async function load() {
  try {
    presets.value = await api.getPresets();
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}
function startEdit() {
  drafts.value = presets.value.map((p) => p.text);
  editing.value = true;
}
function addRow() {
  drafts.value.push("");
}
function removeRow(i: number) {
  drafts.value.splice(i, 1);
}
async function save() {
  try {
    await api.setPresets(drafts.value);
    editing.value = false;
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
function cancel() {
  editing.value = false;
}
function toggleChime() {
  chimeOn.value = !chimeOn.value;
  localStorage.setItem("sd.dir.toastChime", chimeOn.value ? "on" : "off");
}

onMounted(load);
</script>

<template>
  <section class="card">
    <h2>快捷消息</h2>

    <template v-if="!editing">
      <ul class="list">
        <li v-for="p in presets" :key="p.id">{{ p.text }}</li>
        <li v-if="!presets.length" class="empty">无预制消息</li>
      </ul>
      <div class="row">
        <button @click="startEdit">编辑</button>
        <button class="chime" :class="{ on: chimeOn }" @click="toggleChime">
          提示音：{{ chimeOn ? "开" : "关" }}
        </button>
      </div>
    </template>

    <template v-else>
      <div class="edit">
        <div class="erow" v-for="(_, i) in drafts" :key="i">
          <input v-model="drafts[i]" placeholder="消息文本" />
          <button class="del" @click="removeRow(i)">✕</button>
        </div>
        <button class="add" @click="addRow">+ 添加</button>
      </div>
      <div class="row">
        <button @click="save">保存</button>
        <button @click="cancel">取消</button>
      </div>
    </template>

    <p v-if="error" class="err">{{ error }}</p>
  </section>
</template>

<style scoped>
.list {
  list-style: none;
  margin: 0 0 10px;
  padding: 0;
  display: grid;
  gap: 4px;
}
.list li {
  font-family: var(--mono);
  font-size: 12px;
  letter-spacing: 0.04em;
  color: var(--ink-dim);
  padding: 4px 0;
  border-bottom: 1px solid var(--line);
}
.list li.empty {
  color: var(--ink-faint);
  text-transform: uppercase;
  letter-spacing: 0.12em;
}
.row {
  display: flex;
  gap: 8px;
}
.row button {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 8px 10px;
}
.chime.on {
  background: var(--ink);
  color: #000;
  border-color: var(--ink);
}
.edit {
  display: grid;
  gap: 6px;
  margin-bottom: 10px;
}
.erow {
  display: flex;
  gap: 6px;
}
.erow input {
  flex: 1 1 auto;
  background: var(--panel-2);
  border: 1px solid var(--line-hi);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 13px;
  padding: 7px 9px;
}
.del {
  flex: 0 0 auto;
  padding: 0 10px;
}
.add {
  font-family: var(--mono);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  padding: 7px 10px;
}
.err {
  margin: 8px 0 0;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
</style>
