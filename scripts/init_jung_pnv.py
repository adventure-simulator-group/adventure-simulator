#!/usr/bin/env python3
"""Download and verify the pinned Jung/IIASA European PNV v1.1 rasters."""
from __future__ import annotations
import argparse, hashlib, json, os, tempfile, urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path

RECORD = "14627466"
DOI = "10.5281/zenodo.14627466"
BASE = f"https://zenodo.org/api/records/{RECORD}/files"
CHUNK = 1024 * 1024

@dataclass(frozen=True)
class SourceFile:
    filename: str
    size: int
    md5: str
    sha256: str

FILES = (
    SourceFile("pnv_mostlikely_current_laea_1km.tif",4195207,"db680904cec1b046c0c4d1479c3b8cf7","b5c1e48263fe7eb3ef4a7a926605821851b64662dc03a33ea53fec24f56b72eb"),
    SourceFile("pnv_Grassland_current_laea_1km.tif",137352581,"9f271670bdf9abbd636f02da4ac204be","66425840c4993f4ed3c9d8415374c5b11838aa73f38f5d72c10cf35483a24170"),
    SourceFile("pnv_Heathland.and.shrub_current_laea_1km.tif",137713877,"0a094c5f8d5b129845dd9eb1752465f6","dccbe42b114059399b5a272b9956cad2ef37dbcd651ed6b90fa7615958a33923"),
    SourceFile("pnv_Marine.inlets.and.transitional.waters_current_laea_1km.tif",142525264,"9f4f4c6dc435102895ad3e399bbcd8fe","f26b14e2f2a18098a43f561699072f97a7ed9a91308998d0ac9e6796697fe8eb"),
    SourceFile("pnv_Sparsely.vegetated.areas_current_laea_1km.tif",137536892,"86d62568a2befc40885ed5c9e5e5750f","6fb5476ac9eb438c2fd4c36aed270ce8a4092d218281551a02b119cd32ccb92b"),
    SourceFile("pnv_Wetlands_current_laea_1km.tif",137250364,"0cc844d28b230ee6f86ad411532afe78","b4029e292af3fce6d30fda4d7bf72d51abc942665ace836533347c599362b5b2"),
    SourceFile("pnv_Woodland.and.forest_current_laea_1km.tif",138346368,"0361cd4f289a7069d8e17bc00deb5c92","d0850e59de86e5631818eb711747fcd2b1ffcf4e415eb77006fd4f48d509e877"),
)

def hashes(path: Path) -> tuple[int,str,str]:
    md5=hashlib.md5(usedforsecurity=False); sha=hashlib.sha256(); size=0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(CHUNK), b""):
            size += len(block); md5.update(block); sha.update(block)
    return size,md5.hexdigest(),sha.hexdigest()

def valid(path: Path, spec: SourceFile) -> bool:
    return path.is_file() and path.stat().st_size == spec.size and hashes(path)==(spec.size,spec.md5,spec.sha256)

def download(spec: SourceFile, destination: Path) -> None:
    url=f"{BASE}/{spec.filename}/content"
    request=urllib.request.Request(url,headers={"User-Agent":"AdventureSimulator-world-init/1"})
    temporary=None
    try:
        with urllib.request.urlopen(request,timeout=60) as response:
            length=response.headers.get("Content-Length")
            if length is not None and int(length)>spec.size: raise ValueError(f"oversize Content-Length for {spec.filename}")
            fd,name=tempfile.mkstemp(prefix=f".{spec.filename}.",suffix=".part",dir=destination)
            os.close(fd); temporary=Path(name)
            total=0
            with temporary.open("wb") as output:
                while block:=response.read(CHUNK):
                    total += len(block)
                    if total>spec.size: raise ValueError(f"download exceeded pinned size for {spec.filename}")
                    output.write(block)
            if hashes(temporary)!=(spec.size,spec.md5,spec.sha256): raise ValueError(f"checksum or size mismatch for {spec.filename}")
            os.replace(temporary,destination/spec.filename); temporary=None
    finally:
        if temporary is not None: temporary.unlink(missing_ok=True)

def initialize(destination: Path) -> None:
    destination.mkdir(parents=True,exist_ok=True)
    for spec in FILES:
        path=destination/spec.filename
        if not valid(path,spec): download(spec,destination)
    manifest={"schema":1,"record":RECORD,"doi":DOI,"version":"1.1","publication_date":"2025-01-10","license":"CC-BY-4.0","files":[asdict(f)|{"url":f"{BASE}/{f.filename}/content"} for f in FILES]}
    payload=json.dumps(manifest,sort_keys=True,indent=2)+"\n"
    fd,name=tempfile.mkstemp(prefix=".jung-pnv-manifest.",suffix=".tmp",dir=destination); os.close(fd)
    temporary=Path(name)
    try: temporary.write_text(payload,encoding="utf-8"); os.replace(temporary,destination/"jung-pnv-manifest.json")
    finally: temporary.unlink(missing_ok=True)

def main() -> None:
    parser=argparse.ArgumentParser(); parser.add_argument("--output-dir",type=Path,default=Path("target/world-data-sources/raw/jung-pnv")); args=parser.parse_args(); initialize(args.output_dir)
if __name__=="__main__": main()
