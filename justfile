# 暂存全部改动，提交后推送到当前分支的上游
push $message:
    git add -A
    git diff --cached --quiet || git commit -m "$message"
    git -c push.default=upstream push

# 仅快进更新当前分支
pull:
    git pull --ff-only

# 使用现有 release 启动脚本，可追加工程路径
[positional-arguments]
run *args:
    exec scripts/run "$@"
