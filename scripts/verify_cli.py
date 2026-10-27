#!/usr/bin/env python3
"""Check codwawzpc's CLI without game data, plus an optional real import.

Uses private XDG directories and a different working directory, so it also
checks that the CLI does not depend on the shell's current directory. With
CODWAWZPC_TEST_IPA=/path/to/COD_Zombies_1.5.0.ipa it additionally imports,
verifies and re-checks a real copy of the game.
"""
import os
import json
from pathlib import Path
import plistlib
import struct
import subprocess
import tempfile
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "target/release/codwawzpc"


def main():
    with tempfile.TemporaryDirectory(prefix="codwawzpc-cli-") as temporary:
        path = Path(temporary)
        env = dict(os.environ, XDG_DATA_HOME=str(path / "data"),
                   XDG_CONFIG_HOME=str(path / "config"),
                   XDG_STATE_HOME=str(path / "state"))
        cwd = path / "elsewhere"
        cwd.mkdir()

        def run(*args, success=True):
            result = subprocess.run([EXE, *args], cwd=cwd, env=env,
                                    capture_output=True, text=True)
            output = result.stdout + result.stderr
            assert (result.returncode == 0) == success, (args, output)
            return output

        assert "Usage:" in run("--help")
        assert "Usage:" in run()
        assert run("--version").startswith("codwawzpc ")
        assert 'codwawzpc launcher' in run('--help')
        assert 'Usage: codwawzpc launcher' in run('launcher', 'unexpected', success=False)
        # Exercise real launcher dispatch from elsewhere without opening a GUI.
        probe = path / 'python launcher probe'
        probe_output = path / 'probe-output.json'
        probe.write_text('#!/usr/bin/env python3\nimport json, pathlib, sys\n'
                         f'pathlib.Path({str(probe_output)!r}).write_text(json.dumps(sys.argv[1:]))\n')
        probe.chmod(0o700)
        env['CODWAWZPC_PYTHON'] = str(probe)
        run('launcher')
        dispatched = json.loads(probe_output.read_text())
        assert Path(dispatched[0]).resolve() == ROOT / 'launcher/main.py', dispatched
        assert dispatched[1:] == ['--binary', str(EXE)], dispatched
        del env['CODWAWZPC_PYTHON']
        assert "Unknown command" in run("frobnicate", success=False)
        assert "Usage" in run("import", success=False)
        assert "No game has been imported" in run("check", success=False)
        assert "No game has been imported" in run("run", success=False)
        assert '--coop-server=HOST:PORT' in run('--help')
        assert '--player-name=NAME' in run('--help')
        for server in ('', 'localhost:27960', '[::1]:27960'):
            assert 'No game has been imported' in run(
                'run', f'--coop-server={server}', '--player-name=Joueur 日本語', success=False)
        for server in ('host', ':27960', 'host:0', 'host:65536', '::1:27960',
                       '[invalid]:27960', 'https://host:27960'):
            assert 'CoopServer' in run('run', f'--coop-server={server}', success=False)
        assert 'PlayerName' in run('run', '--player-name=', success=False)
        assert 'Unknown run option' in run('run', '--coop-server', success=False)
        for bad in ["--controls=gamepad", "--sensitivity=0", "--sensitivity=nan",
                    "--volume=5", "--resources"]:
            run("run", bad, success=False)
        assert not (path / "data/codwawzpc/current.plist").exists()
        # Saves: Adler-32 of the rest, then the version (1.8.0).
        assert "Usage: codwawzpc export-save" in run("export-save", success=False)
        assert "Usage: codwawzpc import-save" in run("import-save", success=False)
        exported = path / "exported.i3d"
        assert "No saved game yet" in run("export-save", str(exported), success=False)
        payload = bytes([0, 8, 1, 0]) + b"progress"
        save = path / "save.i3d"
        save.write_bytes(zlib.adler32(payload).to_bytes(4, "little") + payload)
        corrupt = path / "corrupt.i3d"
        corrupt.write_bytes(save.read_bytes()[:-1] + b"?")
        assert "checksum" in run("import-save", str(corrupt), success=False)
        assert "no previous save" in run("import-save", str(save))
        assert "backed up to" in run("import-save", str(save))
        assert "save format 1.8.0" in run("export-save", str(exported))
        assert exported.read_bytes() == save.read_bytes()
        assert "Usage: codwawzpc reset-save" in run("reset-save", "now", success=False)
        assert "Backed up to" in run("reset-save")
        assert "No saved game yet" in run("export-save", str(exported), success=False)
        assert "no saved game to reset" in run("reset-save")
        run("import-save", str(save))
        assert "No backup was made" in run("reset-save", "--no-backup")
        assert "No saved game yet" in run("export-save", str(exported), success=False)

        # Correct identity but not the verified payload: rejected, nothing
        # installed.
        ipa = path / "fixture.ipa"
        info = {"CFBundleIdentifier": "com.activision.callofduty",
                "CFBundleVersion": "1.5.0", "CFBundleExecutable": "kiwi"}
        executable = struct.pack("<12I", 0xfeedface, 12, 6, 2, 1, 20, 0, 0x21, 20, 0, 0, 0)
        files = {"Info.plist": plistlib.dumps(info), "kiwi": executable}
        for name in ["_main.dz", "_nacht.dz", "_nacht_full.dz", "_verruckt.dz",
                     "_verruckt_full.dz", "_shinonuma.dz", "_shinonuma_full.dz",
                     "_derriese.dz", "_derriese_full.dz"]:
            files[name] = b"fixture " + name.encode()
        with zipfile.ZipFile(ipa, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, data in files.items():
                archive.writestr(f"Payload/kiwi.app/{name}", data)
        assert "does not match the verified 1.5.0 release" in run(
            "import", str(ipa), success=False)
        assert "No game has been imported" in run("check", success=False)

        real = os.environ.get("CODWAWZPC_TEST_IPA")
        if real:
            out = run("import", real)
            assert "Imported Call of Duty: Zombies 1.5.0" in out, out
            out = run("check")
            assert "Der Riese" in out and "Installation verified" in out, out
            # A failed import keeps the working installation.
            run("import", str(ipa), success=False)
            run("check")
            print("real IPA import verified")

        assert not any(cwd.iterdir()), "the CLI wrote into the working directory"
    print("CLI_VERIFY_PASS")


if __name__ == "__main__":
    main()
