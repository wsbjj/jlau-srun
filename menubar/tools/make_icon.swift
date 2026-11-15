// 生成「校」字 App 图标（1024x1024 PNG）
// 用法: swift tools/make_icon.swift <输出路径.png>
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon.png"
let size: CGFloat = 1024

let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()

// 圆角底
let rect = NSRect(x: 0, y: 0, width: size, height: size)
let radius = size * 0.22
let bg = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)

let gradient = NSGradient(colors: [
    NSColor(calibratedRed: 0.11, green: 0.42, blue: 0.86, alpha: 1.0),
    NSColor(calibratedRed: 0.16, green: 0.62, blue: 0.92, alpha: 1.0),
])!
gradient.draw(in: bg, angle: -90)

// 内描边（增加层次）
NSColor(calibratedWhite: 1.0, alpha: 0.18).setStroke()
bg.lineWidth = size * 0.02
bg.stroke()

// 「校」字
let text = "校" as NSString
let font = NSFont.systemFont(ofSize: size * 0.60, weight: .semibold)
let shadow = NSShadow()
shadow.shadowColor = NSColor(calibratedWhite: 0, alpha: 0.28)
shadow.shadowBlurRadius = size * 0.03
shadow.shadowOffset = NSSize(width: 0, height: -size * 0.012)

let attrs: [NSAttributedString.Key: Any] = [
    .font: font,
    .foregroundColor: NSColor.white,
    .shadow: shadow,
]
let textSize = text.size(withAttributes: attrs)
text.draw(
    at: NSPoint(x: (size - textSize.width) / 2, y: (size - textSize.height) / 2),
    withAttributes: attrs
)

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else {
    FileHandle.standardError.write("渲染失败\n".data(using: .utf8)!)
    exit(1)
}
try png.write(to: URL(fileURLWithPath: out))
print("已生成 \(out)")
