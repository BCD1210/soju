"""Build the experimental bridge against an explicitly supplied OpenBW checkout."""
import argparse
import pathlib
import subprocess

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--stardust", type=pathlib.Path, required=True,
                        help="Existing Stardust checkout with matching OpenBW headers and build/lib")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = args.stardust.resolve()
    api = root / "3rdparty/openbw/bwapi/bwapi"
    includes = [root / "3rdparty", api / "include", api / "Util/Source",
                api / "BWAPI/openbw", api / "BWAPI/openbw/BWAPI", api / "BWAPI/Source",
                api / "BWAPICore", api / "OpenBWData", api / "Shared",
                root / "3rdparty/openbw/openbw"]
    libs = root / "build/lib"
    for file in (api / "include/BWAPI.h", root / "3rdparty/nlohmann/json.hpp",
                 libs / "libBWAPI.dylib", libs / "libBWAPILIB.dylib", libs / "libOpenBWData.dylib"):
        if not file.is_file():
            parser.error(f"Missing dependency: {file}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["clang++", "-std=c++17", "-O2", "-include", "cstdint",
                    *[f"-I{p}" for p in includes],
                    str(pathlib.Path(__file__).with_name("bridge.cpp")),
                    f"-L{libs}", "-lBWAPI", "-lBWAPILIB", "-lOpenBWData",
                    f"-Wl,-rpath,{libs}", "-o", str(args.output)], check=True)

if __name__ == "__main__":
    main()
