$ErrorActionPreference = 'Stop'

$models = @(
  @{ Url = 'https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx'; RelativePath = 'det/ch_PP-OCRv5_det_server.onnx' },
  @{ Url = 'https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_server.onnx'; RelativePath = 'rec/ch_PP-OCRv5_rec_server.onnx' },
  @{ Url = 'https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/rec/en_PP-OCRv5_rec_mobile.onnx'; RelativePath = 'rec/en_PP-OCRv5_rec_mobile.onnx' },
  @{ Url = 'https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.8.0/onnx/PP-OCRv5/cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx'; RelativePath = 'cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx' }
)

$cacheDir = Join-Path (Get-Location) '.rapidocr-manifest-cache'
New-Item -ItemType Directory -Force -Path $cacheDir | Out-Null

foreach ($model in $models) {
  $fileName = Split-Path $model.Url -Leaf
  $target = Join-Path $cacheDir $fileName
  if (-not (Test-Path $target)) {
    Write-Host "Downloading $fileName..."
    Invoke-WebRequest -Uri $model.Url -OutFile $target -MaximumRedirection 10
  }
  $hash = (Get-FileHash -Algorithm SHA256 -Path $target).Hash.ToLowerInvariant()
  $size = (Get-Item $target).Length
  @"
        ModelFile {
            url: "$($model.Url)",
            relative_path: "$($model.RelativePath)",
            sha256: "$hash",
            size: $size,
        },
"@
}

Write-Host "Cached downloads in $cacheDir. Remove that directory when finished."
