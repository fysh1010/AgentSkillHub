# -*- coding: utf-8 -*-
"""把 icon-source.png 缩放出 Tauri 各尺寸图标与 .ico"""
from PIL import Image
import os

SRC = r"D:\Tools\AgentSkillHub\src-tauri\icons\icon-source.png"
OUT = r"D:\Tools\AgentSkillHub\src-tauri\icons"

img = Image.open(SRC).convert("RGBA")

# Tauri Windows/Linux 图标
sizes = {
    "icon.png": 512,
    "128x128@2x.png": 256,
    "128x128.png": 128,
    "64x64.png": 64,
    "32x32.png": 32,
}
for name, size in sizes.items():
    im = img.resize((size, size), Image.LANCZOS)
    im.save(os.path.join(OUT, name))
    print(f"{name} {size}x{size} ok")

# Windows .ico（多尺寸）
im256 = img.resize((256, 256), Image.LANCZOS)
im256.save(
    os.path.join(OUT, "icon.ico"),
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
print("icon.ico ok")

# 也更新 Windows 商店 Logo（NSIS 不需要，但保留一致性）
store_sizes = {
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}
for name, size in store_sizes.items():
    im = img.resize((size, size), Image.LANCZOS)
    im.save(os.path.join(OUT, name))
print("store logos ok")
