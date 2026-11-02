// ocr: Apple Vision text recognition of one image file (stdout = recognized lines, top to bottom). No Tesseract needed on the runner.
// Written for the macOS app-test (appci/cases.py Ctx.ocr uses it when tesseract is absent). Compiled at job start like htype2/kl3; no binaries in git.
// usage: ocr IMAGE.png
import Foundation
import Vision
import CoreGraphics
import ImageIO
guard CommandLine.arguments.count > 1,
      let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: CommandLine.arguments[1]) as CFURL, nil),
      let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else { FileHandle.standardError.write("cannot read image\n".data(using: .utf8)!); exit(2) }
let req = VNRecognizeTextRequest()
req.recognitionLevel = .accurate
req.usesLanguageCorrection = false
let h = VNImageRequestHandler(cgImage: img, options: [:])
do { try h.perform([req]) } catch { FileHandle.standardError.write("vision failed: \(error)\n".data(using: .utf8)!); exit(1) }
let obs = (req.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
for o in obs { if let c = o.topCandidates(1).first { print(c.string) } }
