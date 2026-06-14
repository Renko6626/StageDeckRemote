// 网络与服务状态（spec §11.10 / §6.1.2）：本机 IP、端口、访问地址、token。
import { defineStore } from "pinia";
import { ref } from "vue";

export const useNetworkState = defineStore("network", () => {
  const localIp = ref<string | null>(null);
  const serverPort = ref<number>(8787);
  const token = ref<string>("");
  const accessUrl = ref<string | null>(null);
  /** 与后端最近一次通信是否成功（导播 GUI ↔ Tauri IPC）。 */
  const online = ref<boolean>(false);

  function update(p: {
    localIp: string | null;
    serverPort: number;
    token: string;
    accessUrl: string | null;
  }) {
    localIp.value = p.localIp;
    serverPort.value = p.serverPort;
    token.value = p.token;
    accessUrl.value = p.accessUrl;
  }

  return { localIp, serverPort, token, accessUrl, online, update };
});
