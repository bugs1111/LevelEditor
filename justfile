# 直接运行 just 时列出可用命令
_default:
    @just --list

# 推送到当前分支的上游，提供提交说明时先暂存并提交改动
push $message="":
    @if [ -n "$message" ]; then \
        git add -A && { git diff --cached --quiet || git commit -m "$message"; }; \
    else \
        changes=$(git status --porcelain) && if [ -n "$changes" ]; then \
            printf '%s\n' '本次只推送已有提交。要提交当前改动，请运行 just push "提交说明"。' >&2; \
        fi; \
    fi
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

# 在 Mac 上构建指定平台和模式的便携目录
[positional-arguments]
build *args:
    exec python3 scripts/build.py "$@"
