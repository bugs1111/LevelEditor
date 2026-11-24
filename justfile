# 暂存全部改动，提交后推送到当前分支的上游
push $message:
    git add -A
    git diff --cached --quiet || git commit -m "$message"
    git -c push.default=upstream push

# 仅快进更新当前分支
pull:
    git pull --ff-only

# 启动 debug 版本并显示控制台日志，可追加工程路径
[positional-arguments]
run *args:
    exec scripts/run "$@"

# 用已安装的 Godot 打开 LDtk 横版地图演示
[positional-arguments]
demo-godot *args:
    exec python3 importer/godot/run_demo.py "$@"

# 在 Mac 上构建便携目录，默认 all / release，-a 可选 Mac 架构
[positional-arguments]
build *args:
    exec python3 scripts/build.py "$@"

# 发布当前提交到 main，默认递增补丁版本，可指定稳定版版本号
[positional-arguments]
pub *args:
    exec python3 scripts/publish.py "$@"
