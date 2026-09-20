# Reproducible dependency/input selection; reference runtime is Windows x64 Python 3.11.9.
from pathlib import Path
import sys
from PyInstaller.utils.hooks import copy_metadata

root = Path(SPECPATH)
sys.path.insert(0, str(root))
from bundle_manifest import without_system_ucrt
datas = [(str(root / 'source_binding.js'), '.'), (str(root / 'observer.js'), '.'),
         (str(root / 'licenses'), 'licenses'), (str(root / 'README.md'), '.')]
datas += copy_metadata('frida')
datas += [(str(Path(sys.base_prefix) / 'LICENSE.txt'), 'licenses/python-runtime')]
# Ship matching reviewable source and build inputs, never bytecode caches.
for pattern in ('*.py', '*.js', '*.md', '*.txt', '*.spec', 'tests/*.py', 'tests/*.cjs'):
    for item in sorted(root.glob(pattern)):
        datas.append((str(item), str(Path('source') / item.relative_to(root).parent)))
datas.append((str(root.parents[1] / 'scripts' / 'build-rc003-helper.ps1'), 'source/scripts'))
a = Analysis([str(root / 'helper.py')], pathex=[str(root)], binaries=[], datas=datas,
             hiddenimports=['frida._frida'], hookspath=[], hooksconfig={}, runtime_hooks=[],
             excludes=[], noarchive=False, optimize=0)
# Minimum OS is Windows 10 1809: Windows always uses its system UCRT.
# Filter only that DLL; retain the VC runtime and every API-set forwarder.
a.binaries = without_system_ucrt(a.binaries)
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, [], exclude_binaries=True, name='SayAllKeyHelper',
          debug=False, bootloader_ignore_signals=False, strip=False, upx=False,
          console=False, disable_windowed_traceback=True, uac_admin=False, uac_uiaccess=False)
coll = COLLECT(exe, a.binaries, a.datas, strip=False, upx=False, name='SayAllKeyHelper')
