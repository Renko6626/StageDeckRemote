// 二维码渲染(spec §6.1.2)。离线:qrcode 为纯 JS,打包进 bundle,场馆无网也可用。
import QRCode from "qrcode";

/// 在 canvas 上离线生成二维码(深近黑模块 + 白底,保证手机可扫)。
export async function renderQrCode(text: string, target: HTMLCanvasElement): Promise<void> {
  if (!text) return;
  await QRCode.toCanvas(target, text, {
    width: target.width, // 吃 canvas 的 160
    margin: 1,
    errorCorrectionLevel: "M",
    color: { dark: "#0a0a0bff", light: "#ffffffff" },
  });
}
