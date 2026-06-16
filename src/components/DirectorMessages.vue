<script setup lang="ts">
// 快捷消息预制编辑(导播)+ 点一条发给主持人。
import { ref, onMounted } from "vue";

import { api, type PresetMessage } from "../utils/api";

const presets = ref<PresetMessage[]>([]);
const editing = ref(false);
const drafts = ref<{ id: number; text: string }[]>([]);
const error = ref<string | null>(null);
const sentFlash = ref("");
let flashTimer: number | undefined;
let draftSeq = 0; // monotonic id source for stable :key

async function load() {
  try {
    presets.value = await api.getPresets();
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}
async function sendToPresenter(p: PresetMessage) {
  try {
    await api.sendToPresenter(p.id);
    sentFlash.value = "已发送给主持人 ✓";
  } catch (e) {
    sentFlash.value = "发送失败";
    error.value = String(e);
  }
  if (flashTimer) window.clearTimeout(flashTimer);
  flashTimer = window.setTimeout(() => (sentFlash.value = ""), 1600);
}
function startEdit() {
  drafts.value = presets.value.map((p) => ({ id: draftSeq++, text: p.text }));
  editing.value = true;
}
function addRow() {
  drafts.value.push({ id: draftSeq++, text: "" });
}
function removeRow(i: number) {
  drafts.value.splice(i, 1);
}
async function save() {
  try {
    await api.setPresets(drafts.value.map((d) => d.text));
    editing.value = false;
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
function cancel() {
  editing.value = false;
}

onMounted(load);
</script>

<template>
  <section class="card">
    <h2>快捷消息</h2>

    <template v-if="!editing">
      <div class="hint">点一条发给主持人</div>
      <ul class="list">
        <li v-for="p in presets" :key="p.id">
          <button class="send" @click="sendToPresenter(p)">{{ p.text }}</button>
        </li>
        <li v-if="!presets.length" class="empty">无预制消息</li>
      </ul>
      <p v-if="sentFlash" class="sent">{{ sentFlash }}</p>
      <div class="row">
        <button @click="startEdit">编辑</button>
      </div>
    </template>

    <template v-else>
      <div class="edit">
        <div class="erow" v-for="(row, i) in drafts" :key="row.id">
          <input v-model="row.text" placeholder="消息文本" />
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
.hint {
  font-family: var(--mono);
  font-size: 9px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-faint);
  margin-bottom: 8px;
}
.list {
  list-style: none;
  margin: 0 0 8px;
  padding: 0;
  display: grid;
  gap: 5px;
}
.list li.empty {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-faint);
  text-transform: uppercase;
  letter-spacing: 0.12em;
  padding: 4px 0;
}
.send {
  width: 100%;
  text-align: left;
  font-family: var(--sans);
  font-size: 13px;
  font-weight: 600;
  padding: 9px 11px;
}
.sent {
  margin: 0 0 8px;
  font-family: var(--mono);
  font-size: 11px;
  letter-spacing: 0.08em;
  color: var(--ink);
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
