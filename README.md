# LinkinNotes 四象限浮窗

适用于 Windows x64 的好用便签四象限桌面浮窗，使用 Tauri、React 和 Python 开发。读取全部未完成待办，并通过客户端本地同步队列双向同步。

构建后运行 `release/LinkinNotes/LinkinNotes.exe`，请保持同目录的 `sidecar` 文件夹完整。更新版本时，先从托盘菜单退出旧版，再运行新版。

- 先打开并登录好用便签，浮窗自动读取全部未完成任务，不限制月份。
- 点击象限右上角 + 行内新建；点击任务文字原位编辑；Enter 保存、Shift+Enter 换行、Esc 取消；点击方框完成。
- 右键任务或输入行切换象限，不再弹出任务编辑窗口。
- 保存和完成立即写入好用便签的本地同步队列，由客户端继续同步到云端。
- 每两秒读取客户端变化。底部显示待同步或失败状态。
- 顶部可拖动、切换置顶、最大化/还原或隐藏；系统托盘菜单可以重新显示或退出。
- 不在 Windows 任务栏显示应用图标，保留系统托盘图标；也可通过快捷键显示/隐藏。
- 任务列表上下固定保留 8px 留白，滚动时文字不会贴住象限边框。
- 左上角三横杠打开设置，可自定义四个象限名称和显示/隐藏快捷键（默认 Ctrl+Alt+Q）。窗口在前台时按下隐藏，隐藏、最小化或不在前台时按下唤起。快捷键被占用时会提示，保留原设置。
- 设置保存在系统用户目录下的应用配置文件中，重启后保留；象限名称仅影响浮窗显示。
- 窗口聚焦和失焦均保持不透明，避免文字变淡；保留圆角窗口。
- 富文本/附件任务可调整象限，内容请在原客户端编辑；重复任务请在原客户端完成，以保留后续提醒。
- 发生并发修改时保留输入，采用最新版本前请核对内容。切换账号后重新打开编辑框。

当前适配好用便签 Windows 8.0.5 的本地数据库结构。密钥仅在内存中使用，不保存账号密码或明文数据库。应用更新后如本地结构改变，需要重新验证适配。

## 从源码构建

需要 Windows x64、Node.js、Rust MSVC 工具链、Visual Studio C++ Build Tools、Python 3.12 和 WebView2 Runtime。

```powershell
npm ci
python -m pip install -r backend/requirements.txt
./build.ps1
```

构建脚本从上游下载并校验固定版本的原生依赖，运行后端测试，再打包前端、原生窗口和 Python sidecar。输出位于 `release/LinkinNotes/`。

单独运行检查：

```powershell
python scripts/fetch_native_dependencies.py
python -m unittest discover -s backend -v
npm run build
cargo test --release --manifest-path src-tauri/Cargo.toml
```

测试在临时目录创建纯虚构数据库，覆盖中文内容、索引触发器、冲突拒绝、不按月份过滤及事务回滚，不读取或修改真实待办。

## 数据与实现

- 通过当前用户已登录的好用便签进程确定账号和数据库，仅在内存中读取并使用数据库密钥。
- 修改使用 SQLite 原生事务，并写入 `operations_v1`，由好用便签客户端继续同步。
- 用户配置保存在 `%APPDATA%/local.linkin.notes/preferences.json`，不包含在仓库中。
- 本项目是独立的第三方工具，与好用便签官方无隶属关系；请仅连接自己有权访问的账号和数据。

原生依赖版本、下载来源及许可链接见 [THIRD_PARTY.md](THIRD_PARTY.md)。仓库不包含第三方 DLL、个人待办数据库、研究记录或已编译程序。
