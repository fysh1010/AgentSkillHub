"""生成 AgentSkillHub 应用图标源图（方案 A · 枢纽分发）。

设计：品牌渐变圆角底 + 中心圆点（技能库） + 三条射线到三个节点（多个 Agent）。
4 倍超采样保证边缘抗锯齿。

用法: python _gen_icon.py
输出: src-tauri/icons/icon-source-new.png   (1024x1024 RGBA)
      src-tauri/icons/icon-preview-256.png  (小尺寸辨识度自检)
"""
import os

import numpy as np
from PIL import Image, ImageDraw

N = 1024          # 输出尺寸
SS = 4            # 超采样倍数
W = N * SS        # 绘制尺寸
UNIT = 150.0      # 设计坐标系边长（与矢量草图一致）

C1 = (0x4E, 0x6E, 0xF2)   # 品牌蓝 #4e6ef2
C2 = (0x7C, 0x5C, 0xFC)   # 品牌紫 #7c5cfc

ICONS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src-tauri", "icons")
OUT = os.path.join(ICONS, "icon-source-new.png")

# 设计坐标（150 单位制，已做视觉居中）
# 三个节点以 120 度均匀分布：一个正上方、两个在下方左右对称
SPOKE = 50.0                  # 中心到节点的距离
HUB_R = 19.0                  # 中心枢纽半径（明显大于节点，形成主次关系）
DOT_R = 8.5                   # 节点半径
LINE_W = 4.5                  # 射线线宽（细一些，"连接"语义更清晰）
CORNER_R = 34.0               # 底圆角
HUB = (75.0, 87.5)            # 中心枢纽（下移使图形视觉居中）
DOTS = (
    (75.0, HUB[1] - SPOKE),
    (75.0 + SPOKE * 0.8660254, HUB[1] + SPOKE * 0.5),
    (75.0 - SPOKE * 0.8660254, HUB[1] + SPOKE * 0.5),
)


def main() -> None:
    s = W / UNIT

    # 1. 对角渐变底（smoothstep 过渡更柔和）
    y, x = np.mgrid[0:N, 0:N].astype(np.float32)
    t = (x + y) / (2.0 * (N - 1))
    t = t * t * (3.0 - 2.0 * t)
    c1 = np.array(C1, dtype=np.float32)
    c2 = np.array(C2, dtype=np.float32)
    base = c1[None, None, :] * (1.0 - t[..., None]) + c2[None, None, :] * t[..., None]

    # 2. 超采样绘制白色图形 -> LANCZOS 缩小得到抗锯齿 alpha
    big = Image.new("L", (W, W), 0)
    d = ImageDraw.Draw(big)
    hx, hy = HUB[0] * s, HUB[1] * s
    pts = [(px * s, py * s) for px, py in DOTS]
    lw = max(1, int(round(LINE_W * s)))
    for px, py in pts:
        d.line([hx, hy, px, py], fill=255, width=lw)
    dot_r = DOT_R * s
    for px, py in pts:
        d.ellipse([px - dot_r, py - dot_r, px + dot_r, py + dot_r], fill=255)
    hub_r = HUB_R * s
    d.ellipse([hx - hub_r, hy - hub_r, hx + hub_r, hy + hub_r], fill=255)

    mask = np.asarray(big.resize((N, N), Image.LANCZOS)).astype(np.float32) / 255.0

    # 3. 白色叠加到渐变底
    out = base * (1.0 - mask[..., None]) + 255.0 * mask[..., None]
    img = Image.fromarray(out.astype(np.uint8), "RGB").convert("RGBA")

    # 4. 圆角方形遮罩
    rmask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(rmask).rounded_rectangle(
        [0, 0, N - 1, N - 1], radius=int(round(CORNER_R * N / UNIT)), fill=255
    )
    img.putalpha(rmask)

    img.save(OUT)
    print("written:", OUT, img.size, img.mode)

    prev = img.resize((256, 256), Image.LANCZOS)
    p256 = os.path.join(ICONS, "icon-preview-256.png")
    prev.save(p256)
    print("written:", p256, prev.size)


if __name__ == "__main__":
    main()
