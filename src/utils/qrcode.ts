// 二维码渲染（spec §6.1.2）。
//
// MVP 占位实现：当前只把链接以文本形式展示，并预留接入点。
// TODO: 接入离线二维码库（如 `qrcode` npm 包，`pnpm add qrcode`），
//       在 canvas 上离线生成二维码，避免依赖外网服务。
export async function renderQrCode(text: string, target: HTMLCanvasElement): Promise<void> {
  const ctx = target.getContext("2d");
  if (!ctx) return;
  const size = target.width;
  ctx.fillStyle = "#1c1c24";
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = "#6a6a82";
  ctx.font = "13px system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("二维码占位", size / 2, size / 2 - 8);
  ctx.fillText("（待接入 qrcode 库）", size / 2, size / 2 + 12);
  // 链接本身在面板里以文本形式给出，可直接手动输入。
  void text;
}
