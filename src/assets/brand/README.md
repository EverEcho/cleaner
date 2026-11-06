# AppClear 图标

`appclear.png` 是应用主图标源，使用内置 image_gen 生成。Dock、ICNS、ICO 和各尺寸 PNG 由 `pnpm tauri icon src/assets/brand/appclear.png` 生成。`tray-mark.svg` 是对应的单色菜单栏标识，以模板图像自动适配 macOS 深浅菜单栏。

菜单栏 PNG 的生成：

```
pnpm tauri icon src/assets/brand/tray-mark.svg --output /tmp/appclear-tray-icons --png 44
cp /tmp/appclear-tray-icons/44x44.png src-tauri/icons/tray-template.png
```

主图标生成提示词：

> Use case: logo-brand. Asset type: production macOS app icon for AppClear, a premium local disk cleaner and system monitor. Create ONE centered app icon on genuinely transparent canvas, square 1024x1024. A softly rounded continuous squircle tile occupying about 82% of canvas with comfortable even transparent padding. Deep violet to luminous periwinkle satin gradient, subtle top-left light, refined dimensional edge, clean modern Apple-quality craft. Center a bold simple pearly-white sculpted C ribbon / open circular sweep, with one small crisp four-point cleaning sparkle near the C opening at upper right, a tiny cool mint accent only in that sparkle. Geometric minimal silhouette, visually memorable and legible at 16px, clean uncluttered. Large confident white mark. Premium utility, calm, precise. No text, no letters besides the abstract C mark, no numbers, no broom, no trash bin, no gear, no shields, no photo scenery, no surrounding device or mockup, no grid, no second icon, no watermark. Smooth precise edges, no decorative particles, no heavy drop shadow.
