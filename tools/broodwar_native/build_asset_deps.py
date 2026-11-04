"""Build pinned local readers for the user's installed CASC and owned SCM/SCX files."""
import pathlib
import subprocess
from client import RUNTIME

VERSIONS={
    'StormLib':'116135c27e8c0dcf8ea0a0c549a6d737ce10fc73',
    'CascLib':'6b889ec8f421cbc9baccec296c582362ab917887',
}

def build():
    root=RUNTIME/'deps';root.mkdir(parents=True,exist_ok=True)
    for name,revision in VERSIONS.items():
        source=root/name
        if not source.exists():
            subprocess.run(['git','clone','--no-checkout','https://github.com/ladislav-zezula/'+name+'.git',str(source)],check=True)
            subprocess.run(['git','checkout','--detach',revision],cwd=source,check=True)
        actual=subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip()
        if actual!=revision:raise RuntimeError(f'{name} revision differs; existing checkout left unchanged')
        if name=='StormLib':
            cmake=source/'CMakeLists.txt';text=cmake.read_text()
            old='target_link_options(${LIBRARY_NAME} PRIVATE "-framework Carbon")'
            new='target_link_options(${LIBRARY_NAME} PRIVATE "SHELL:-framework Carbon")'
            if old in text:cmake.write_text(text.replace(old,new))
            elif new not in text:raise RuntimeError('Unexpected StormLib macOS linker configuration')
        args=['cmake','-S',str(source),'-B',str(source/'build'),'-DCMAKE_BUILD_TYPE=Release']
        if name=='StormLib':args+=['-DBUILD_SHARED_LIBS=ON','-DSTORM_USE_BUNDLED_LIBRARIES=ON']
        else:args+=['-DCMAKE_POLICY_VERSION_MINIMUM=3.5','-DCASC_BUILD_SHARED_LIB=ON','-DCASC_BUILD_STATIC_LIB=OFF']
        subprocess.run(args,check=True)
        subprocess.run(['cmake','--build',str(source/'build'),'-j','4'],check=True)

if __name__=='__main__':build()
