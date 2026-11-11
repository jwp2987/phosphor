// ocrw: Apple Vision text recognition of one image file WITH word boxes, for placing the pointer (#2061, r14-test-gaps T32). ocr.swift prints lines
// only and stays as it is. Compiled by the job like the other tools (script/realapp/mac/ci/app_test.sh); no binaries in git.
// usage: ocrw IMAGE.png
// stdout: one line per whitespace-separated word, top to bottom: "word<TAB>x<TAB>y<TAB>w<TAB>h" in IMAGE PIXELS, top-left origin (the same space
// as tesseract's tsv; appci/macptr.py vision_words turns it into cases_ui.words() rows).
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
let W = Double(img.width), H = Double(img.height)
let obs = (req.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
for o in obs {
  guard let c = o.topCandidates(1).first else { continue }
  let s = c.string
  var i = s.startIndex
  while i < s.endIndex {
    while i < s.endIndex, s[i].isWhitespace { i = s.index(after: i) }
    var j = i
    while j < s.endIndex, !s[j].isWhitespace { j = s.index(after: j) }
    if i < j {
      // the box of this word inside the recognised line; the line's own box when Vision gives none for the range
      let box = try? c.boundingBox(for: i..<j)
      let b: CGRect = box?.boundingBox ?? o.boundingBox
      let x = b.minX * W, y = (1 - b.maxY) * H, w = b.width * W, hh = b.height * H
      print("\(s[i..<j])\t\(Int(x.rounded()))\t\(Int(y.rounded()))\t\(Int(w.rounded()))\t\(Int(hh.rounded()))")
    }
    i = j
  }
}
