/**
 * 生成 Tauri 更新所需的 latest.json 文件
 * 用法: node scripts/generate-update-json.cjs <版本号> <安装包路径>
 * 示例: node scripts/generate-update-json.cjs 0.1.1 ./src-tauri/target/release/bundle/nsis/MeowMic_0.1.1_x64-setup.exe
 *
 * latest.json 会直接生成到安装包同目录，自动读取 .sig 签名文件（如有）
 */

const fs = require('fs');
const path = require('path');

function main() {
  const args = process.argv.slice(2);

  if (args.length < 2) {
    console.log('用法: node scripts/generate-update-json.cjs <版本号> <安装包路径>');
    console.log('示例: node scripts/generate-update-json.cjs 0.1.1 ./src-tauri/target/release/bundle/nsis/MeowMic_0.1.1_x64-setup.exe');
    process.exit(1);
  }

  const version = args[0];
  const installerPath = args[1];

  if (!fs.existsSync(installerPath)) {
    console.error('错误: 安装包不存在:', installerPath);
    process.exit(1);
  }

  // 获取文件大小
  const stats = fs.statSync(installerPath);

  // 尝试读取同目录的 .sig 签名文件
  const sigPath = installerPath + '.sig';
  let signature = '需要私钥签名后填入（使用 pnpm tauri signer sign <安装包路径>）';
  if (fs.existsSync(sigPath)) {
    signature = fs.readFileSync(sigPath, 'utf8').trim();
    console.log('✅ 已读取签名文件:', sigPath);
  }

  // 生成 latest.json 结构
  const updateJson = {
    version: version,
    notes: "新版本更新",
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature,
        url: `https://github.com/NanCheng-L/MeowMic/releases/download/v${version}/MeowMic_${version}_x64-setup.exe`
      }
    }
  };

  // 输出到安装包同目录
  const outputPath = path.join(path.dirname(installerPath), 'latest.json');
  fs.writeFileSync(outputPath, JSON.stringify(updateJson, null, 2));

  console.log('✅ latest.json 已生成:', outputPath);
  console.log('\n文件信息:');
  console.log('  版本:', version);
  console.log('  文件大小:', (stats.size / 1024 / 1024).toFixed(2), 'MB');
  console.log('  文件路径:', installerPath);
  console.log('\n⚠️  发布步骤:');
  console.log('  1. 在 GitHub 创建 Release v' + version);
  console.log('  2. 上传安装包 + latest.json 到 Release');
}

main();
