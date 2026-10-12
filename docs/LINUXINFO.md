# Linux 基本信息插件

`plugins/linuxinfo` 是仓库唯一内置的分析插件。插件 ID 为 `org.lovelycarve.linuxinfo`，检材级 analyzer 为 `linuxbaseinfo`，版本以 `plugin.toml` 为准。`plugins/sdk` 是 C ABI / LPP SDK，`plugins/host` 是进程隔离加载器。

## 构建与部署

```text
just build-plugins
just build-plugins --release
just pack-plugins
```

构建把 `linuxinfo.dll`、`liblinuxinfo.so` 或 `liblinuxinfo.dylib` 复制到 `plugin.toml` 旁。桌面安装包和 CLI 包部署 `plugins/linuxinfo/`，将 `lc-plugin-host` 放在主程序旁。`pack-plugins` 为 Windows 生成带宿主、许可文本、来源提交信息和 SHA-256 校验值的 ZIP，输出在 `dist-pack/`。

修改后退出并重新启动应用。发现的插件均视为可信代码，隔离进程不提供操作系统权限沙箱。部署和生命周期见 [插件协议](PLUGIN_PROTOCOL.md)。

## 分析范围

在目标检材的「任务中心 → 快速分析」中选择 Linux 平台和「Linux信息」。插件遍历具有 Linux 根标志的全部卷，包括 Docker 合并卷，执行清单声明的十五个子项。

| 数据 | 主要来源 |
| --- | --- |
| 系统、时区 | os-release、发行版标志、hostname、timezone / localtime |
| 用户、用户组 | passwd、shadow、group |
| 历史命令、用户痕迹 | Bash / Zsh 历史、GTK XBEL 最近访问 |
| 登录、登录失败、开关机 | wtmp / btmp 的 utmp 记录 |
| 网络 | NetworkManager、OpenWrt UCI、RedHat ifcfg、Hosts 与 DHCP 租约 |
| USB | var/log 中的设备相关文本记录 |
| 服务、启动项 | systemd 单元与 target、init.d |
| 软件 | DPKG status、RPM rpmdb.sqlite |
| 定时任务 | crontab 与用户 cron 文件 |

结果保留来源卷和文件。登录记录保留偏移并按 PID / 终端配对登出；同名服务的不同来源分别保留。GTK 最近访问保留应用、命令、次数与时间；网络租约保留地址、DNS、网关和租期。配置登记、计划时间与实际运行事件分别解释，无时区日志不伪装为 UTC。

展示列由 `presentation.rs` 根据完整结果集生成，分页或筛选不改变列。记录身份放在 `artifact.title`；全空列省略，0 和 False 保留。来源定位通过 `source.root` / `source.path` 访问原证据，不要求先完成全盘索引。

## 读取与验证

`evidence.rs` 提供有大小预算的完整文件读取：缺失路径和非文件返回无内容，超限及短读报错。子项的普通解析错误保留诊断；短读、资源上限和取消使任务失败。宿主与结果受 LPP 上限约束。

```text
just fmt
just clippy-plugins
just test-plugins
just build-plugins
just run-example probe_plugin <镜像> org.lovelycarve.linuxinfo linuxbaseinfo <结果.json>
just run-example probe_plugin_sources <镜像> <结果.json>
```

可选真实镜像回归：设置 `LOVELYCARVE_TEST_LINUX_IMAGE` 为本机 Linux 镜像绝对路径，再运行 `just test linuxinfo_plugin_analyzes_real_image -- --ignored --nocapture`。验证应覆盖加载、读取、结果落库、分页、来源定位与取消。私有检材和生成结果不提交仓库。
