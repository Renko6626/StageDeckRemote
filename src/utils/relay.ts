/** 输入预览与保存共用；先换协议再解析，避免 HTTPS 默认端口被提前丢掉。 */
export function normalizeRelayAddress(input: string): string {
  const raw = input.trim();
  const parts = raw.split("://");
  if (parts.length > 2 || (parts.length === 2 && !["http", "https", "ws", "wss"].includes(parts[0].toLowerCase()))) {
    throw new Error("请填写 HTTP、HTTPS、WS 地址或主机与端口");
  }
  let url: URL;
  try {
    url = new URL("ws://" + (parts.length === 2 ? parts[1] : raw));
  } catch {
    throw new Error("服务器地址无效，请填写主机和端口");
  }
  if (!url.hostname || url.pathname !== "/" || url.search || url.hash || url.username || url.password) {
    throw new Error("只填写服务器主机和端口，不要包含扫码路径、查询参数或账号");
  }
  return url.href.replace(/\/$/, "");
}

export function relayStatusText(status: string): string {
  if (status === "connected") return "Relay 已连接";
  if (status === "connecting") return "Relay 连接中";
  if (status.startsWith("error:")) return "Relay 连接失败";
  return "Relay 未启用";
}
