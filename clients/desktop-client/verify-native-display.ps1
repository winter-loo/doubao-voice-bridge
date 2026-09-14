# Read-only display routing inventory. Does not create windows, capture pixels,
# change topology/policy, stop software, or enable/disable/install any driver.
# API basis: QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS), DisplayConfigGetDeviceInfo,
# EnumDisplayDevicesW and GetSystemPowerStatus. Public Microsoft SDK layouts.
[CmdletBinding()]
param([switch]$CompileOnly)
$ErrorActionPreference = 'Stop'
if (-not ('GlassDisplayInventory27' -as [type])) {
Add-Type -ReferencedAssemblies System.Drawing,System.Windows.Forms -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class GlassDisplayInventory27 {
    [StructLayout(LayoutKind.Sequential)] struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)] struct Ratio { public uint N,D; }
    [StructLayout(LayoutKind.Sequential)] struct Source {
        public Luid Adapter; public uint Id,ModeIndex,Status;
    }
    [StructLayout(LayoutKind.Sequential)] struct Target {
        public Luid Adapter; public uint Id,ModeIndex,Technology,Rotation,Scaling;
        public Ratio Refresh; public uint ScanLine; public int Available; public uint Status;
    }
    [StructLayout(LayoutKind.Sequential)] struct DisplayPath {
        public Source Source; public Target Target; public uint Flags;
    }
    // The SDK MODE_INFO contains a 48-byte union after its 16-byte header.
    // This inventory does not interpret that union; its storage must still exist.
    [StructLayout(LayoutKind.Explicit,Size=64)] struct Mode {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(16)] public ulong UnionAlignment;
    }
    [StructLayout(LayoutKind.Sequential)] struct Header {
        public uint Type,Size; public Luid Adapter; public uint Id;
    }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct SourceName {
        public Header Header;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string Name;
    }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct TargetName {
        public Header Header; public uint Flags,Technology;
        public ushort Manufacturer,Product; public uint Connector;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=64)] public string FriendlyName;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string DevicePath;
    }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct AdapterName {
        public Header Header;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string DevicePath;
    }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct DisplayDevice {
        public uint Size;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string Name;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string Description;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string Id;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=128)] public string Key;
    }
    [StructLayout(LayoutKind.Sequential)] struct Power {
        public byte Ac,Battery,Percent,Saver;
        public uint Remaining,Full;
    }
    [DllImport("user32.dll",ExactSpelling=true)] static extern int GetDisplayConfigBufferSizes(uint flags,out uint paths,out uint modes);
    [DllImport("user32.dll",ExactSpelling=true)] static extern int QueryDisplayConfig(uint flags,ref uint count,[Out] DisplayPath[] paths,ref uint modeCount,[Out] Mode[] modes,IntPtr topology);
    [DllImport("user32.dll",EntryPoint="DisplayConfigGetDeviceInfo",ExactSpelling=true)] static extern int GetSource(ref SourceName value);
    [DllImport("user32.dll",EntryPoint="DisplayConfigGetDeviceInfo",ExactSpelling=true)] static extern int GetTarget(ref TargetName value);
    [DllImport("user32.dll",EntryPoint="DisplayConfigGetDeviceInfo",ExactSpelling=true)] static extern int GetAdapter(ref AdapterName value);
    [DllImport("user32.dll",CharSet=CharSet.Unicode,ExactSpelling=true)] [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool EnumDisplayDevicesW(string name,uint index,ref DisplayDevice device,uint flags);
    [DllImport("kernel32.dll",ExactSpelling=true,SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool GetSystemPowerStatus(out Power power);

    public sealed class Route {
        public string GdiName,AdapterDescription,SourceAdapterLuid,TargetAdapterLuid;
        public string SourceHardware,TargetHardware,MonitorModel,Connector;
        public uint SourceId,TargetId,ConnectorCode;
        public bool TargetAvailable,IndirectConnector;
        public bool? Primary,AttachedToDesktop;
        public double? RefreshHz;
    }
    public sealed class Inventory {
        public string Scope="Active display routes only, not the adapter selected by GPUI/DWM and not proof of a driver defect. Hardware instance IDs/monitor serials are not emitted.";
        public Route[] ActivePaths; public int BatterySaverFlag; public string[] Warnings;
    }
    static void CheckSize(Type type,int size) {
        if(Marshal.SizeOf(type)!=size) throw new InvalidOperationException("SDK layout mismatch: "+type.Name);
    }
    public static void ValidateLayouts() {
        CheckSize(typeof(DisplayPath),72); CheckSize(typeof(Mode),64);
        CheckSize(typeof(SourceName),84); CheckSize(typeof(TargetName),420);
        CheckSize(typeof(AdapterName),276); CheckSize(typeof(DisplayDevice),840);
        CheckSize(typeof(Power),12);
    }
    static Header Request(uint type,Type structure,Luid adapter,uint id) {
        return new Header{Type=type,Size=(uint)Marshal.SizeOf(structure),Adapter=adapter,Id=id};
    }
    static string Key(Luid id) { return unchecked((uint)id.High).ToString("X8")+":"+id.Low.ToString("X8"); }
    static string Hardware(Luid id,List<string> warnings) {
        var name=new AdapterName{Header=Request(4,typeof(AdapterName),id,0)};
        int code=GetAdapter(ref name);
        if(code!=0){warnings.Add("GetAdapter "+Key(id)+": "+code);return null;}
        // Interface path: \\?\BUS#HARDWARE-ID#INSTANCE#{GUID}. Do NOT emit instance.
        string[] parts=(name.DevicePath??"").Split('#');
        if(parts.Length<2){warnings.Add("Adapter path format unrecognized; omitted.");return null;}
        string bus=parts[0];int p=bus.LastIndexOf('\\');
        return (p<0?bus:bus.Substring(p+1))+"\\"+parts[1];
    }
    static string Connector(uint code) {
        switch(code) {
            case 0:return "VGA";case 4:return "DVI";case 5:return "HDMI";
            case 6:return "LVDS";case 10:return "DisplayPort external";
            case 11:return "DisplayPort embedded";case 15:return "Miracast";
            case 16:return "Indirect wired";case 17:return "Indirect virtual";
            case 18:return "DisplayPort USB tunnel";case 0x80000000:return "Internal";
            default:return "Other/SDK code "+code;
        }
    }
    public static Inventory Read() {
        ValidateLayouts(); const uint flags=2; // ONLY_ACTIVE_PATHS; no topology mutation.
        var warnings=new List<string>();
        var devices=new Dictionary<string,DisplayDevice>(StringComparer.OrdinalIgnoreCase);
        for(uint i=0;i<64;i++){
            var d=new DisplayDevice{Size=(uint)Marshal.SizeOf(typeof(DisplayDevice))};
            if(!EnumDisplayDevicesW(null,i,ref d,0))break;
            if(!String.IsNullOrEmpty(d.Name))devices[d.Name]=d;
            if(i==63)warnings.Add("Display-device label enumeration reached its limit.");
        }
        for(int attempt=0;attempt<4;attempt++){
            uint count,modes;int code=GetDisplayConfigBufferSizes(flags,out count,out modes);
            if(code!=0)throw new Win32Exception(code,"GetDisplayConfigBufferSizes failed");
            if(count<1||count>64||modes>512)throw new InvalidOperationException("Unexpected active display buffer size.");
            var paths=new DisplayPath[count];var modeArray=new Mode[Math.Max(modes,1u)];
            code=QueryDisplayConfig(flags,ref count,paths,ref modes,modeArray,IntPtr.Zero);
            if(code==122)continue; // Topology changed: retry buffer-size query, not set topology.
            if(code!=0)throw new Win32Exception(code,"QueryDisplayConfig failed");
            var routes=new List<Route>();
            for(int i=0;i<count;i++){
                var path=paths[i];
                var source=new SourceName{Header=Request(1,typeof(SourceName),path.Source.Adapter,path.Source.Id)};
                var target=new TargetName{Header=Request(2,typeof(TargetName),path.Target.Adapter,path.Target.Id)};
                int sc=GetSource(ref source),tc=GetTarget(ref target);
                if(sc!=0)warnings.Add("GetSource: "+sc);
                if(tc!=0)warnings.Add("GetTarget: "+tc);
                var row=new Route{
                    GdiName=sc==0?source.Name:null,MonitorModel=tc==0?target.FriendlyName:null,
                    SourceId=path.Source.Id,TargetId=path.Target.Id,
                    SourceAdapterLuid=Key(path.Source.Adapter),TargetAdapterLuid=Key(path.Target.Adapter),
                    SourceHardware=Hardware(path.Source.Adapter,warnings),TargetHardware=Hardware(path.Target.Adapter,warnings),
                    Connector=Connector(path.Target.Technology),ConnectorCode=path.Target.Technology,
                    IndirectConnector=path.Target.Technology==16||path.Target.Technology==17,
                    TargetAvailable=path.Target.Available!=0,
                    RefreshHz=path.Target.Refresh.D==0?(double?)null:Math.Round((double)path.Target.Refresh.N/path.Target.Refresh.D,3)
                };
                DisplayDevice display;
                if(row.GdiName!=null&&devices.TryGetValue(row.GdiName,out display)){
                    row.AdapterDescription=display.Description;
                    row.Primary=(display.Flags&4)!=0;row.AttachedToDesktop=(display.Flags&1)!=0;
                }else warnings.Add("GDI adapter label not resolved for active route.");
                routes.Add(row);
            }
            Power power;bool havePower=GetSystemPowerStatus(out power);
            if(!havePower)warnings.Add("GetSystemPowerStatus: "+Marshal.GetLastWin32Error());
            return new Inventory{ActivePaths=routes.ToArray(),BatterySaverFlag=havePower?power.Saver:-1,Warnings=warnings.ToArray()};
        }
        throw new InvalidOperationException("Display topology kept changing; no stable inventory returned.");
    }
}
'@
}
[GlassDisplayInventory27]::ValidateLayouts()
if ($CompileOnly) { Write-Output 'Display probe compiled and SDK layouts checked; no display API query or settings change.'; $global:LASTEXITCODE=0; return }
$warnings = @()
$cpu = @()
$models = @()
try { $cpu = @(Get-CimInstance Win32_Processor -Property Name -OperationTimeoutSec 15 | ForEach-Object { $_.Name }) } catch { $warnings += 'CPU model query unavailable.' }
try { $models = @(Get-CimInstance Win32_ComputerSystem -Property Manufacturer,Model -OperationTimeoutSec 15 | ForEach-Object { [ordered]@{manufacturer=$_.Manufacturer;model=$_.Model} }) } catch { $warnings += 'System model query unavailable.' }
$inventory = [GlassDisplayInventory27]::Read()
[ordered]@{display=$inventory;cpu_model=$cpu;computer_model=$models;additional_warnings=$warnings}|ConvertTo-Json -Depth 5 -Compress
$global:LASTEXITCODE=0
