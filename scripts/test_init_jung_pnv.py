import hashlib, io, sys, tempfile, unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).parent))
import init_jung_pnv as init

class Response(io.BytesIO):
    def __init__(self,data,length=None): super().__init__(data); self.headers={} if length is None else {"Content-Length":str(length)}
    def __enter__(self): return self
    def __exit__(self,*_): self.close()

class InitializerTests(unittest.TestCase):
    def spec(self,data=b"source"):
        return init.SourceFile("fixture.tif",len(data),hashlib.md5(data,usedforsecurity=False).hexdigest(),hashlib.sha256(data).hexdigest())
    def test_atomic_verified_download(self):
        with tempfile.TemporaryDirectory() as root, patch.object(init.urllib.request,"urlopen",return_value=Response(b"source",6)):
            init.download(self.spec(),Path(root)); self.assertEqual((Path(root)/"fixture.tif").read_bytes(),b"source")
            self.assertEqual(list(Path(root).glob("*.part")),[])
    def test_oversize_content_length_leaves_no_file(self):
        with tempfile.TemporaryDirectory() as root, patch.object(init.urllib.request,"urlopen",return_value=Response(b"source",7)):
            with self.assertRaises(ValueError): init.download(self.spec(),Path(root))
            self.assertEqual(list(Path(root).iterdir()),[])
    def test_stream_cap_and_checksum_reject_bad_content(self):
        for data in (b"source!",b"xxxxxx"):
            with self.subTest(data=data), tempfile.TemporaryDirectory() as root, patch.object(init.urllib.request,"urlopen",return_value=Response(data)):
                with self.assertRaises(ValueError): init.download(self.spec(),Path(root))
                self.assertEqual(list(Path(root).iterdir()),[])
    def test_existing_wrong_size_is_rejected_before_hashing(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/"fixture.tif"; path.write_bytes(b"too long")
            with patch.object(init,"hashes",side_effect=AssertionError("must not hash wrong-size cache")):
                self.assertFalse(init.valid(path,self.spec()))
if __name__=="__main__": unittest.main()
