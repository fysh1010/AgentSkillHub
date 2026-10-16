"""兼容旧入口：从 public/app-icon.svg 生成全部图标。"""
from pathlib import Path
import subprocess


def main() -> None:
    root = Path(__file__).resolve().parents[2]
    subprocess.run(["node", str(root / "scripts" / "generate-icons.mjs")], cwd=root, check=True)


if __name__ == "__main__":
    main()
