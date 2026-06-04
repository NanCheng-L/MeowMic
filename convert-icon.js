const fs = require('fs');
const path = require('path');

// Since we can't use sharp/canvas in this environment,
// we'll create a simple script that generates a placeholder
// and instructions for manual conversion

const iconDir = path.join(__dirname, 'src-tauri', 'icons');
const svgPath = path.join(iconDir, 'icon.svg');

// Read the SVG
const svgContent = fs.readFileSync(svgPath, 'utf8');
console.log('SVG file loaded:', svgPath);
console.log('SVG size:', svgContent.length, 'bytes');

// Create a simple HTML file that can be opened in browser to convert SVG to PNG
const conversionHtml = `<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>SVG to PNG Converter</title>
    <style>
        body { font-family: sans-serif; padding: 20px; background: #1a1a2e; color: #10b981; }
        .container { max-width: 800px; margin: 0 auto; }
        .preview { background: #0a0a0f; padding: 20px; border-radius: 8px; margin: 20px 0; }
        .buttons { display: flex; gap: 10px; flex-wrap: wrap; }
        button { background: #10b981; color: #0a0a0f; border: none; padding: 10px 20px; border-radius: 6px; cursor: pointer; font-weight: bold; }
        button:hover { background: #059669; }
        .size { font-size: 14px; margin-top: 5px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>pico-denoise Icon Converter</h1>
        <p>Click buttons below to download PNG icons at different sizes:</p>
        <div class="preview">
            <img id="svg" src="icon.svg" style="max-width: 200px;">
        </div>
        <div class="buttons">
            <button onclick="download(32)">Download 32x32</button>
            <button onclick="download(128)">Download 128x128</button>
            <button onclick="download(256)">Download 256x256 (128@2x)</button>
            <button onclick="download(512)">Download 512x512</button>
        </div>
        <p class="size">After downloading, rename files to match Tauri requirements</p>
    </div>
    <script>
        function download(size) {
            const canvas = document.createElement('canvas');
            canvas.width = size;
            canvas.height = size;
            const ctx = canvas.getContext('2d');
            const img = document.getElementById('svg');
            ctx.drawImage(img, 0, 0, size, size);
            const link = document.createElement('a');
            link.download = size + 'x' + size + '.png';
            link.href = canvas.toDataURL('image/png');
            link.click();
        }
    </script>
</body>
</html>`;

fs.writeFileSync(path.join(iconDir, 'converter.html'), conversionHtml);
console.log('\nCreated converter.html in icons directory');
console.log('\nTo convert SVG to PNG:');
console.log('1. Open src-tauri/icons/converter.html in a browser');
console.log('2. Click buttons to download PNGs at each size');
console.log('3. Save them as:');
console.log('   - 32x32.png');
console.log('   - 128x128.png');
console.log('   - 128x128@2x.png (256x256)');
console.log('   - icon.png (512x512)');
console.log('\nFor ICO file, use an online ICO converter with the 256x256 PNG');
