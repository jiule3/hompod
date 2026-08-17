HomePod Bridge - GitHub 编译 EXE 上传说明

你需要上传的是本 ZIP 解压后的全部内容，不是 ZIP 文件本身。

GitHub 仓库根目录最终至少应看到：
  .github/
  apps/
  crates/
  docs/
  scripts/
  tests/
  Cargo.toml
  README.md
  LICENSE
  THIRD_PARTY_NOTICES.md

最简单步骤：
1. 下载 HomePod-Bridge-GitHub-Build.zip
2. 在电脑上解压
3. 打开你的 GitHub 空仓库
4. Add file -> Upload files
5. 将解压目录中的全部文件/文件夹上传到仓库根目录并 Commit changes
6. 打开 Actions -> Build HomePod Bridge EXE -> Run workflow
7. 构建完成后，在该次运行页面底部 Artifacts 下载 HomePod-Bridge-Windows-x64
8. 解压 Artifact，里面应有：
   HomePod-Bridge.exe
   HomePod-Bridge-Setup.exe

注意：不要只上传 HomePod-Bridge-GitHub-Build.zip，因为 GitHub Actions 不会自动解压仓库中的 ZIP 作为源码。
