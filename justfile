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

# 用已安装的 Godot 打开地图导入演示
[positional-arguments]
demo-godot *args:
    exec godot --editor --path importer/godot "$@"

# 在 Mac 上构建指定平台和模式的便携目录
[positional-arguments]
build *args:
    exec python3 scripts/build.py "$@"
