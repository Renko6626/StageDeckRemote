//! 帧 broker:一处编码、watch 扇出最新编码帧;采样 hash 做变化检测;MJPEG 分帧。
use std::sync::Arc;
use tokio::sync::watch;

/// 已编码的一帧(JPEG)。seq 单调递增,消费者据此判新。
#[derive(Debug)]
pub struct EncodedFrame {
    pub seq: u64,
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 对缓冲做采样 FNV-1a hash(变化检测用)。只取 ~1024 个等距采样点,
/// 静止时哪怕 WGC 高频来帧,这步也几乎零成本;幻灯片切页是大面积变化,采样必然命中。
pub fn sample_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let n = bytes.len();
    if n == 0 {
        return h;
    }
    let samples = 1024usize.min(n);
    // 等距采样 samples 个点:第 k 点取 idx = k * n / samples(覆盖 [0, n))。
    // 用乘法定位而非 i += stride,避免整除把步长截断后采样点偏移、漏掉缓冲中段。
    let mut k = 0;
    while k < samples {
        let idx = k * n / samples;
        h ^= bytes[idx] as u64;
        h = h.wrapping_mul(0x100000001b3);
        k += 1;
    }
    // 末字节兜底(stride 跨过尾部时仍纳入长度敏感)
    h ^= (n as u64).wrapping_mul(0x100000001b3);
    h
}

/// 把一段 JPEG 包成一个 multipart/x-mixed-replace 帧段。
pub fn mjpeg_part(jpeg: &[u8]) -> Vec<u8> {
    let header = format!(
        "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
        jpeg.len()
    );
    let mut out = Vec::with_capacity(header.len() + jpeg.len() + 2);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(jpeg);
    out.extend_from_slice(b"\r\n");
    out
}

/// watch 扇出最新编码帧。多消费者订阅;慢消费者自动只看最新(无积压)。
pub struct FrameBroker {
    tx: watch::Sender<Arc<EncodedFrame>>,
}

impl FrameBroker {
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(Arc::new(EncodedFrame {
            seq: 0,
            jpeg: Vec::new(),
            width: 0,
            height: 0,
        }));
        Self { tx }
    }

    /// 发布一帧(seq 自增)。用 send_replace:即便当前无订阅者也写入最新值,
    /// 这样 latest() 始终反映最近一帧(watch::send 在无订阅者时返回 Err 且不更新存值)。
    pub fn publish(&self, jpeg: Vec<u8>, width: u32, height: u32) {
        let seq = self.tx.borrow().seq.wrapping_add(1); // 临时 borrow 在本语句末即 drop,不与 send 争锁
        self.tx.send_replace(Arc::new(EncodedFrame { seq, jpeg, width, height }));
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<EncodedFrame>> {
        self.tx.subscribe()
    }

    pub fn latest(&self) -> Arc<EncodedFrame> {
        self.tx.borrow().clone()
    }
}

impl Default for FrameBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_hash_is_deterministic_and_sensitive() {
        let a = vec![7u8; 100_000];
        let mut b = a.clone();
        assert_eq!(sample_hash(&a), sample_hash(&a)); // 同缓冲同值
        b[50_000] = b[50_000].wrapping_add(1);        // 改采样命中区一字节
        assert_ne!(sample_hash(&a), sample_hash(&b));
        assert_eq!(sample_hash(&[]), sample_hash(&[])); // 空不 panic
    }

    #[test]
    fn mjpeg_part_frames_with_headers() {
        let part = mjpeg_part(&[0xAA, 0xBB, 0xCC]);
        let text = String::from_utf8_lossy(&part);
        assert!(text.starts_with("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: 3\r\n\r\n"));
        assert_eq!(&part[part.len() - 5..], &[0xAA, 0xBB, 0xCC, b'\r', b'\n']); // 数据 + 收尾 CRLF
    }

    #[test]
    fn broker_publish_then_latest_and_subscribe_get_newest() {
        let broker = FrameBroker::new();
        assert_eq!(broker.latest().seq, 0); // 初始占位帧 seq 0
        broker.publish(vec![1, 2, 3], 4, 4);
        broker.publish(vec![9, 9], 8, 8);
        let latest = broker.latest();
        assert_eq!((latest.seq, latest.jpeg.clone(), latest.width), (2, vec![9, 9], 8));
        // 新订阅者立即可读最新帧
        let rx = broker.subscribe();
        assert_eq!(rx.borrow().seq, 2);
    }
}
