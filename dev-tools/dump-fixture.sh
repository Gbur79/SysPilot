#!/usr/bin/env bash
# [SRE-TOOLING: Host Fixture Dumper for SysPilot Universalism]
# Dumps sanitized host environment metadata as clean JSON.
set -Eeuo pipefail

output=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        -o|--output)
            output="$2"
            shift 2
            ;;
        *)
            if [[ -z "$output" ]]; then
                output="$1"
                shift
            else
                echo "Usage: $0 [--output <file>]" >&2
                exit 1
            fi
            ;;
    esac
done

python3 -c '
import os, glob, json, sys

os_release = {}
for path in ["/etc/os-release", "/usr/lib/os-release"]:
    if os.path.exists(path):
        with open(path) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    os_release[k] = v.strip("\"'\''")
        break

kernel = ""
if os.path.exists("/proc/sys/kernel/osrelease"):
    with open("/proc/sys/kernel/osrelease") as f:
        kernel = f.read().strip()

modules_dir = {
    "running_kernel_dir_exists": os.path.isdir(f"/usr/lib/modules/{kernel}") if kernel else False,
    "has_modules_dep": os.path.isfile(f"/usr/lib/modules/{kernel}/modules.dep") if kernel else False,
}

nvidia_proc = None
if os.path.exists("/proc/driver/nvidia/version"):
    with open("/proc/driver/nvidia/version") as f:
        nv_ver = f.read().strip()
    gpus = {}
    for gpu_info in sorted(glob.glob("/proc/driver/nvidia/gpus/*/information")):
        bdf = os.path.basename(os.path.dirname(gpu_info))
        with open(gpu_info) as f:
            gpus[bdf] = f.read().strip()
    nvidia_proc = {"version": nv_ver, "gpus": gpus}

pci_devices = []
for dev in sorted(glob.glob("/sys/bus/pci/devices/*")):
    class_path = os.path.join(dev, "class")
    if os.path.exists(class_path):
        with open(class_path) as f:
            cls = f.read().strip()
        if cls.lower().startswith("0x03"):
            bdf = os.path.basename(dev)
            vendor = ""
            device = ""
            driver = ""
            vendor_path = os.path.join(dev, "vendor")
            if os.path.exists(vendor_path):
                with open(vendor_path) as f:
                    vendor = f.read().strip()
            device_path = os.path.join(dev, "device")
            if os.path.exists(device_path):
                with open(device_path) as f:
                    device = f.read().strip()
            driver_link = os.path.join(dev, "driver")
            if os.path.islink(driver_link):
                driver = os.path.basename(os.readlink(driver_link))
            pci_devices.append({
                "bdf": bdf,
                "class": cls,
                "vendor": vendor,
                "device": device,
                "driver": driver
            })

icd_files = []
for icd_path in sorted(glob.glob("/usr/share/vulkan/icd.d/*.json")):
    fname = os.path.basename(icd_path)
    try:
        with open(icd_path) as f:
            data = json.load(f)
            icd_files.append({
                "file": fname,
                "content": data
            })
    except Exception:
        pass

candidate_libs = [
    "/usr/lib/libGLX_nvidia.so.0",
    "/usr/lib/libvulkan.so",
    "/usr/lib32/libGLX_nvidia.so.0",
    "/usr/lib32/libvulkan.so",
]
libraries = {p: os.path.exists(p) for p in candidate_libs}

data = {
    "os_release": os_release,
    "kernel": kernel,
    "modules_dir": modules_dir,
    "pci_display_devices": pci_devices,
    "vulkan_icd": icd_files,
    "libraries": libraries
}
if nvidia_proc is not None:
    data["nvidia_proc"] = nvidia_proc

out_arg = sys.argv[1] if len(sys.argv) > 1 else ""
rendered = json.dumps(data, indent=2) + "\n"

if out_arg:
    os.makedirs(os.path.dirname(os.path.abspath(out_arg)), exist_ok=True)
    with open(out_arg, "w") as f:
        f.write(rendered)
else:
    sys.stdout.write(rendered)
' "${output}"
