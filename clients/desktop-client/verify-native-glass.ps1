# Windows-only, preview-only desktop audit. No microphone, hotkey, clipboard,
# desktop capture upload, global settings changes or production process control.
# The script draws its own nonactivating test paper and saves only that small area.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('NativeMaterialAudit13' -as [type])) {
Add-Type -ReferencedAssemblies System.Drawing,System.Windows.Forms -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;
using System.Diagnostics;
using System.Collections.Generic;
using System.Text.RegularExpressions;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public static class NativeMaterialAudit13 {
    [StructLayout(LayoutKind.Sequential)] struct R { public int L,T,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct P { public int X,Y; }
    delegate bool EnumProc(IntPtr h,IntPtr p);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb,IntPtr p);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int ht,uint flags);
    [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h,IntPtr r);
    [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int l,int t,int r,int b);
    [DllImport("gdi32.dll")] static extern bool PtInRegion(IntPtr r,int x,int y);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(P p);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint flags);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h,IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dst,int x,int y,int w,int h,IntPtr src,int sx,int sy,uint rop);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    public sealed class Case {
        public string Name,Requested,Selected,Error,StartupTail;
        public bool Captured,Stopped,HostVisualReported;
        public int OutsidePixels,OutsideChanged,InsidePixels,InsideChanged;
        public int[] OuterPixels;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved;
        public string Directory,Error,ExecutableSha256;
        public string VisualAcceptance="pending image review; pixel changes alone do not prove blur";
        public Case[] Cases;
    }
    sealed class Paper : Form {
        readonly bool dark;
        public Paper(bool darkMode) {
            dark=darkMode; FormBorderStyle=FormBorderStyle.None; ShowInTaskbar=false;
            TopMost=true; StartPosition=FormStartPosition.Manual;
            AutoScaleMode=AutoScaleMode.None; DoubleBuffered=true;
        }
        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams {
            get { var p=base.CreateParams; p.ExStyle|=0x08000080; return p; }
        }
        protected override void OnPaint(PaintEventArgs e) {
            e.Graphics.Clear(dark ? Color.FromArgb(24,28,36) : Color.White);
            using(var f=new Font("Segoe UI",18,FontStyle.Regular,GraphicsUnit.Pixel))
                for(int y=0;y<ClientSize.Height;y+=25)
                    e.Graphics.DrawString("This is text ABC 123456. This is text.",f,dark?Brushes.White:Brushes.Black,4,y);
        }
    }
    static void Check(bool ok,string message) { if(!ok) throw new InvalidOperationException(message); }
    static uint Owner(IntPtr h) { uint p; GetWindowThreadProcessId(h,out p); return p; }
    static IntPtr Root(int x,int y) { return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2); }
    static IntPtr Find(uint pid) {
        IntPtr result=IntPtr.Zero; int count=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused) {
            if(Owner(h)==pid && IsWindowVisible(h)) {
                var s=new StringBuilder(256); GetWindowText(h,s,s.Capacity);
                if(s.ToString()=="Doubao Glass Preview - no microphone") { result=h; ++count; }
            }
            return true;
        };
        Check(EnumWindows(cb,IntPtr.Zero)&&count<=1,"Cannot uniquely enumerate the preview.");
        return result;
    }
    static void Pump(int ms) {
        var t=Stopwatch.StartNew();
        do { Application.DoEvents(); System.Threading.Thread.Sleep(10); } while(t.ElapsedMilliseconds<ms);
    }
    static void Stop(Process p,IntPtr h) {
        if(p==null||p.HasExited) return;
        if(h!=IntPtr.Zero && Owner(h)==(uint)p.Id) PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)) { p.Kill(); Check(p.WaitForExit(5000),"The test preview did not exit."); }
    }
    static Bitmap Shot(Rectangle crop,IntPtr expected,IntPtr paper,Point center,string path) {
        Check(Root(center.X,center.Y)==expected && Root(crop.Left+3,crop.Top+3)==paper,
            "Another window covers the synthetic test area; capture refused.");
        var image=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try {
            using(var g=Graphics.FromImage(image)) {
                IntPtr src=GetDC(IntPtr.Zero), dst=IntPtr.Zero;
                try {
                    Check(src!=IntPtr.Zero,"Cannot access screen DC."); dst=g.GetHdc();
                    Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"Capture failed.");
                } finally { if(dst!=IntPtr.Zero) g.ReleaseHdc(dst); if(src!=IntPtr.Zero) ReleaseDC(IntPtr.Zero,src); }
            }
            Check(Root(center.X,center.Y)==expected,"Window layering changed during capture.");
            image.Save(path,ImageFormat.Png); return image;
        } catch { image.Dispose(); throw; }
    }
    static void Measure(Bitmap image,Bitmap baseline,IntPtr region,Rectangle outer,Rectangle crop,Case result) {
        Point[] offsets={new Point(3,0),new Point(-3,0),new Point(0,3),new Point(0,-3)};
        for(int y=0;y<image.Height;y++) for(int x=0;x<image.Width;x++) {
            int rx=x+crop.Left-outer.Left, ry=y+crop.Top-outer.Top;
            bool inside=PtInRegion(region,rx,ry), edge=false;
            foreach(var d in offsets) if(PtInRegion(region,rx+d.X,ry+d.Y)!=inside) edge=true;
            if(edge) continue;
            var a=image.GetPixel(x,y); var b=baseline.GetPixel(x,y);
            bool changed=Math.Max(Math.Abs(a.R-b.R),Math.Max(Math.Abs(a.G-b.G),Math.Abs(a.B-b.B)))>4;
            if(inside) { result.InsidePixels++; if(changed) result.InsideChanged++; }
            else { result.OutsidePixels++; if(changed) result.OutsideChanged++; }
        }
    }
    static Case RunCase(string exe,string directory,string backend,string theme) {
        var result=new Case{Name=backend+"-"+theme,Requested=backend,Selected="not-reported"};
        Process preview=null; Paper paper=null; Bitmap image=null;
        IntPtr h=IntPtr.Zero,region=IntPtr.Zero;
        System.Threading.Tasks.Task<string> logs=null,output=null;
        try {
            string args="--glass-backend="+backend+" --theme="+theme+" --content=optimizing --seconds=30";
            var si=new ProcessStartInfo(exe,args) { UseShellExecute=false,CreateNoWindow=true,
                WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true };
            // Change only this test process's environment, not the user's shell.
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");
            preview=Process.Start(si); Check(preview!=null,"Could not start test preview.");
            logs=preview.StandardError.ReadToEndAsync(); output=preview.StandardOutput.ReadToEndAsync();
            var wait=Stopwatch.StartNew();
            while(wait.ElapsedMilliseconds<12000 && !preview.HasExited) { h=Find((uint)preview.Id); if(h!=IntPtr.Zero) break; Pump(50); }
            Check(h!=IntPtr.Zero&&!preview.HasExited,"Preview window did not appear."); Pump(1500);
            R r; Check(GetWindowRect(h,out r),"Cannot read preview geometry.");
            var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected preview size.");
            result.OuterPixels=new int[]{outer.X,outer.Y,outer.Width,outer.Height};
            var crop=Rectangle.Intersect(Rectangle.Inflate(outer,24,24),Screen.FromHandle(h).Bounds);
            Check(crop.Contains(outer)&&crop.Left<outer.Left&&crop.Top<outer.Top,"Insufficient crop margin.");
            var center=new Point(outer.Left+outer.Width/2,outer.Top+outer.Height/2);
            paper=new Paper(theme=="dark"); paper.Bounds=Rectangle.Inflate(crop,12,12); paper.Show();
            Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack test preview.");
            Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack test paper.");
            paper.Refresh(); Pump(1500);
            region=CreateRectRgn(0,0,0,0);
            Check(region!=IntPtr.Zero&&GetWindowRgn(h,region)>1,"No capsule region installed.");
            image=Shot(crop,h,paper.Handle,center,Path.Combine(directory,result.Name+".png"));
            Stop(preview,h); Pump(300);
            using(var baseline=Shot(crop,paper.Handle,paper.Handle,center,Path.Combine(directory,result.Name+"-background.png")))
                Measure(image,baseline,region,outer,crop,result);
            result.Captured=true;
        } catch(Exception e) { result.Error=e.Message; }
        finally {
            try { Stop(preview,h); } catch(Exception e) { result.Error=(result.Error??"")+" Cleanup: "+e.Message; }
            if(paper!=null) { paper.Close(); paper.Dispose(); Application.DoEvents(); }
            if(image!=null) image.Dispose(); if(region!=IntPtr.Zero) DeleteObject(region);
            result.Stopped=preview==null||preview.HasExited;
            if(result.Stopped && logs!=null && logs.Wait(2000)) {
                string text=logs.Result??"";
                File.WriteAllText(Path.Combine(directory,result.Name+".log"),text,Encoding.UTF8);
                var matches=Regex.Matches(text,@"\[glass\] requested=\w+ selected=(\w+)");
                if(matches.Count>0) result.Selected=matches[matches.Count-1].Groups[1].Value;
                result.HostVisualReported=text.Contains("[glass-host] native brush attached");
                result.StartupTail=text.Length<=450?text:text.Substring(text.Length-450);
            }
            if(result.Stopped && output!=null) output.Wait(2000);
            if(preview!=null) preview.Dispose();
        }
        return result;
    }
    public static Report Run(string exe,string directory) {
        Check(IntPtr.Size==8,"Use 64-bit Windows PowerShell.");
        Check(File.Exists(exe),"GlassPreview.exe is missing; build it first.");
        var existing=Process.GetProcessesByName("GlassPreview"); int count=existing.Length;
        foreach(var p in existing) p.Dispose(); Check(count==0,"Close existing test previews first; none was stopped.");
        Check(!Directory.Exists(directory),"Output directory already exists; refusing to overwrite it.");
        var result=new Report{Directory=directory}; var cases=new List<Case>();
        IntPtr focus=GetForegroundWindow(),old=IntPtr.Zero;
        try {
            Directory.CreateDirectory(directory);
            old=SetThreadDpiAwarenessContext(new IntPtr(-4)); Check(old!=IntPtr.Zero,"Cannot enter physical-pixel DPI context.");
            foreach(string theme in new string[]{"light","dark"}) foreach(string backend in new string[]{"native","solid"}) {
                var item=RunCase(exe,directory,backend,theme); cases.Add(item);
                Check(item.Captured&&item.Stopped&&item.Error==null,item.Error??"Test case did not complete.");
            }
            result.Completed=true;
        } catch(Exception e) { result.Error=e.Message; }
        finally {
            if(old!=IntPtr.Zero) SetThreadDpiAwarenessContext(old);
            result.FocusPreserved=focus==GetForegroundWindow(); result.Cases=cases.ToArray();
        }
        return result;
    }
}
'@
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $env:TEMP ('doubao-native-audit-' + [Guid]::NewGuid().ToString('N'))
}
$exe = Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
$result = [NativeMaterialAudit13]::Run($exe,$OutputDirectory)
$result.ExecutableSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$json = $result | ConvertTo-Json -Depth 5 -Compress
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),$json)
}
Write-Output $json
# Never emit PNG/Base64 data here: it can exceed the bridge's output limit.
# Completed means the measurements ran, NOT that blur or readability passed.
if (-not $result.Completed) { throw ('Preview audit failed: ' + $result.Error) }
$global:LASTEXITCODE = 0
