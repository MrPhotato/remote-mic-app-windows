# 无线麦 SayAll Windows 宣传短片

64 秒，1920×1080，24 fps，H.264 / AAC。中文画面字幕、原创遥控器插画和电子节奏配乐，无口播；适用于科技和 vibe coding 圈的横屏介绍。

这是一条**交互动效演示**，不是实体遥控器或第三方识别结果的实录。没有展示未验证功能已经通过的测试结果。语音场景注明 VB-CABLE 和目标应用配置要求；三键增强注明实验性、管理员 Helper 及 RC003 验证边界。

## 内容与依据

| 时段 | 内容 |
| --- | --- |
| 00–05 | 指挥 AI，还得抱着键盘？ |
| 05–11 | 遥控器变成 Agent 控制器 |
| 11–17 | Windows 适配与补全，保留上游来源 |
| 17–24 | 语音按下开始、释放结束 |
| 24–32 | 音量＋/－切换聊天或标签页 |
| 32–40 | 普通返回删除，TV 长按撤销 |
| 40–48 | 单击／双击／长按预设与自由定制 |
| 48–54 | RC003 返回及音量三键增强 |
| 54–64 | GPL、GitHub、Mac 原版、Windows 上游与制作署名 |

- 本项目：[MrPhotato/remote-mic-app-windows](https://github.com/MrPhotato/remote-mic-app-windows)。
- Mac 原版：[HD838A/remote-mic-app](https://github.com/HD838A/remote-mic-app)，2026-09-21 核对官方仓库 README。
- Windows 上游：[GetSayAll/remote-mic-app-windows](https://github.com/GetSayAll/remote-mic-app-windows)，不是把已有 Windows 适配归为本次首创。
- 默认键位依据 `src/lib/coding-profile.ts`；验收范围依据 `Testing/WindowsRc003Input.md` 与 `Testing/evidence/final-profile-switch-install-20260921.json`。
- 署名按用户要求：**该 Windows 版本由 GPT-6 Astra Ultra 协同制作**。不表示 OpenAI 官方发行或认可。

## 重建

```powershell
python -m pip install Pillow numpy imageio-ffmpeg==0.6.0
python marketing/promo/render.py --out target/promo
```

Windows 的微软雅黑和 Bahnschrift 仅用于本机渲染，不随仓库分发字体。所有图形由本脚本绘制；音乐由正弦波、包络和固定种子噪声合成，没有外部音乐、视频、人物或商业素材。脚本随仓库以 GPL-3.0-only 提供。

`--preview` 只生成分镜图；完整运行生成 MP4、封面、9 张分镜图和原创 WAV。输出位于 Git 忽略目录，不会随源码推送或自动上传。

## 发布文案

> 指挥 AI，还得抱着键盘？我们把小米蓝牙遥控器变成了 Windows 上的语音与快捷操作入口：按住说话，音量键切换聊天或标签页，返回键连续删除，TV 长按撤销。一套能直接上手的默认键位，也能按单击、双击和长按自由改。基于 SayAll 开源项目继续适配和补全，代码全部开放。当前仍为预览体验，具体设备与第三方应用兼容性请看仓库说明。

附上上面的三个 GitHub 链接，并保留完整验收边界；不要把动效示意标题改成“全功能真机演示”。
