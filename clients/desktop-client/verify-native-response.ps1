# Probe only: fixed-theme previews over a controlled, changing background.
# No microphone, hotkey, clipboard, production process or global setting changes.
[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('NativeResponseProbe14' -as [type])) {
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
public static class NativeResponseProbe14 {
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
    [DllImport("gdi32.dll")] static extern int GetRgnBox(IntPtr r,out R b);
    [DllImport("gdi32.dll")] static extern bool PtInRegion(IntPtr r,int x,int y);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(P p);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint flags);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h,IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dst,int x,int y,int w,int ht,IntPtr src,int sx,int sy,uint rop);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    public sealed class Case {
        public string Name,Selected,Error;
        public bool Stopped,HostReported;
        public int ProbePixels;
        public double CoarseResponse,CoarseReturnDrift,CoarseInput,CoarseGain;
        public double FineResponse,FineReturnDrift,FineInput,FineGain;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved;
        public string Directory,Error,ExecutableSha256;
        public string Interpretation="Gains are measured responses, not automatic acceptance. Compare Native to Solid and return drift; no FPS claim.";
        public Case[] Cases;
    }
    sealed class Paper : Form {
        public int Pattern;
        public int SplitScreenX;
        public Paper() {
            FormBorderStyle=FormBorderStyle.None; ShowInTaskbar=false; TopMost=true;
            StartPosition=FormStartPosition.Manual; AutoScaleMode=AutoScaleMode.None; DoubleBuffered=true;
        }
        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams {
            get { var p=base.CreateParams; p.ExStyle|=0x08000080; return p; }
        }
        protected override void OnPaint(PaintEventArgs e) {
            if(Pattern<2) {
                Color a=Color.FromArgb(220,48,64),b=Color.FromArgb(32,112,224);
                if(Pattern==1) { Color t=a;a=b;b=t; }
                e.Graphics.Clear(a);
                int split=Math.Max(0,Math.Min(ClientSize.Width,SplitScreenX-Left));
                using(var brush=new SolidBrush(b)) e.Graphics.FillRectangle(brush,split,0,ClientSize.Width-split,ClientSize.Height);
            } else {
                // 2 physical-pixel stripes, inverted in the second sample.
                e.Graphics.Clear(Color.White);
                for(int x=Pattern==2?0:2;x<ClientSize.Width;x+=4)
                    e.Graphics.FillRectangle(Brushes.Black,x,0,2,ClientSize.Height);
            }
        }
    }
    static void Check(bool ok,string m) { if(!ok) throw new InvalidOperationException(m); }
    static uint Owner(IntPtr h) { uint p; GetWindowThreadProcessId(h,out p);return p; }
    static IntPtr Root(int x,int y) { return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2); }
    static IntPtr Find(uint pid) {
        IntPtr result=IntPtr.Zero; int n=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused) {
            if(Owner(h)==pid&&IsWindowVisible(h)) {
                var s=new StringBuilder(256);GetWindowText(h,s,s.Capacity);
                if(s.ToString()=="Doubao Glass Preview - no microphone") { result=h;++n; }
            }return true;
        };
        Check(EnumWindows(cb,IntPtr.Zero)&&n<=1,"Preview enumeration failed.");return result;
    }
    static void Pump(int ms) {
        var t=Stopwatch.StartNew();
        do { Application.DoEvents();System.Threading.Thread.Sleep(10); } while(t.ElapsedMilliseconds<ms);
    }
    static void Stop(Process p,IntPtr h) {
        if(p==null||p.HasExited)return;
        if(h!=IntPtr.Zero&&Owner(h)==(uint)p.Id)PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)) { p.Kill();Check(p.WaitForExit(5000),"Preview did not exit."); }
    }
    static Bitmap Shot(Rectangle crop,IntPtr expected,IntPtr paper,Point center,string path) {
        Check(Root(center.X,center.Y)==expected&&Root(crop.Left+3,crop.Top+3)==paper,"Another window covers the test area.");
        var image=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try {
            using(var g=Graphics.FromImage(image)) {
                IntPtr src=GetDC(IntPtr.Zero),dst=IntPtr.Zero;
                try {
                    Check(src!=IntPtr.Zero,"Screen DC unavailable.");dst=g.GetHdc();
                    Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"Capture failed.");
                }finally { if(dst!=IntPtr.Zero)g.ReleaseHdc(dst);if(src!=IntPtr.Zero)ReleaseDC(IntPtr.Zero,src); }
            }
            Check(Root(center.X,center.Y)==expected,"Window layering changed.");image.Save(path,ImageFormat.Png);return image;
        }catch { image.Dispose();throw; }
    }
    static double Difference(Bitmap a,Bitmap b,Bitmap repeat,List<Point> points) {
        double sum=0;
        foreach(var p in points) {
            Color x=a.GetPixel(p.X,p.Y),y=b.GetPixel(p.X,p.Y),z=repeat==null?x:repeat.GetPixel(p.X,p.Y);
            sum+=Math.Abs((x.R+z.R)/2.0-y.R)+Math.Abs((x.G+z.G)/2.0-y.G)+Math.Abs((x.B+z.B)/2.0-y.B);
        }return Math.Round(sum/(points.Count*3),4);
    }
    static Case RunCase(string exe,string directory,string backend,string theme) {
        var result=new Case{Name=backend+"-"+theme,Selected="not-reported"};
        Process preview=null;Paper paper=null;IntPtr h=IntPtr.Zero,region=IntPtr.Zero;
        var shots=new List<Bitmap>();var backgrounds=new List<Bitmap>();
        System.Threading.Tasks.Task<string> logs=null,output=null;
        try {
            var si=new ProcessStartInfo(exe,"--glass-backend="+backend+" --theme="+theme+" --content=optimizing --seconds=60") {
                UseShellExecute=false,CreateNoWindow=true,WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true
            };
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");
            preview=Process.Start(si);Check(preview!=null,"Cannot start preview.");
            logs=preview.StandardError.ReadToEndAsync();output=preview.StandardOutput.ReadToEndAsync();
            var wait=Stopwatch.StartNew();
            while(wait.ElapsedMilliseconds<12000&&!preview.HasExited) { h=Find((uint)preview.Id);if(h!=IntPtr.Zero)break;Pump(50); }
            Check(h!=IntPtr.Zero&&!preview.HasExited,"Preview did not appear.");Pump(1500);
            R r;Check(GetWindowRect(h,out r),"Cannot read geometry.");
            var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected preview size.");
            var screen=Screen.FromHandle(h).Bounds;var crop=Rectangle.Inflate(outer,24,24);
            Check(screen.Contains(crop),"Insufficient monitor margin.");
            region=CreateRectRgn(0,0,0,0);Check(region!=IntPtr.Zero&&GetWindowRgn(h,region)>1,"No capsule region.");
            R box;Check(GetRgnBox(region,out box)>1,"Cannot read capsule bounds.");
            double cx=(box.L+box.Right)/2.0,cy=(box.T+box.Bottom)/2.0,w=box.Right-box.L,ht=box.Bottom-box.T;
            var center=new Point(outer.Left+(int)cx,outer.Top+(int)cy);
            var points=new List<Point>();
            for(int y=box.T;y<box.Bottom;y++)for(int x=box.L;x<box.Right;x++) {
                double dx=Math.Abs(x+0.5-cx),dy=Math.Abs(y+0.5-cy);
                // Side patches exclude foreground label, top/bottom rims and antialiasing.
                if(dx>w*0.34&&dx<w*0.42&&dy<ht*0.16&&PtInRegion(region,x,y))
                    points.Add(new Point(x+outer.Left-crop.Left,y+outer.Top-crop.Top));
            }
            Check(points.Count>=40,"Too few safe probe pixels.");result.ProbePixels=points.Count;
            paper=new Paper();paper.SplitScreenX=center.X;
            paper.Bounds=Rectangle.Intersect(Rectangle.Inflate(crop,220,220),screen);paper.Show();
            Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack preview.");
            Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack paper.");
            int[] patterns={0,1,0,2,3,2};string[] names={"coarse-a","coarse-b","coarse-a-return","fine-a","fine-b","fine-a-return"};
            for(int i=0;i<patterns.Length;i++) {
                paper.Pattern=patterns[i];paper.Refresh();Pump(800);
                R now;Check(!preview.HasExited&&GetWindowRect(h,out now)&&now.L==r.L&&now.T==r.T&&now.Right==r.Right&&now.Bottom==r.Bottom,"Preview geometry changed.");
                shots.Add(Shot(crop,h,paper.Handle,center,Path.Combine(directory,result.Name+"-"+names[i]+".png")));
            }
            Stop(preview,h);Pump(300);
            for(int i=0;i<4;i++) {
                paper.Pattern=i;paper.Refresh();Pump(250);
                backgrounds.Add(Shot(crop,paper.Handle,paper.Handle,center,Path.Combine(directory,result.Name+"-input-"+i+".png")));
            }
            result.CoarseResponse=Difference(shots[0],shots[1],shots[2],points);
            result.CoarseReturnDrift=Difference(shots[0],shots[2],null,points);
            result.FineResponse=Difference(shots[3],shots[4],shots[5],points);
            result.FineReturnDrift=Difference(shots[3],shots[5],null,points);
            result.CoarseInput=Difference(backgrounds[0],backgrounds[1],null,points);
            result.FineInput=Difference(backgrounds[2],backgrounds[3],null,points);
            Check(result.CoarseInput>80&&result.FineInput>200,"Background patterns did not appear as intended.");
            result.CoarseGain=Math.Round(result.CoarseResponse/result.CoarseInput,4);
            result.FineGain=Math.Round(result.FineResponse/result.FineInput,4);
        }catch(Exception e) { result.Error=e.Message; }
        finally {
            try{Stop(preview,h);}catch(Exception e){result.Error=(result.Error??"")+" Cleanup: "+e.Message;}
            if(paper!=null){paper.Close();paper.Dispose();Application.DoEvents();}
            foreach(var b in shots)b.Dispose();foreach(var b in backgrounds)b.Dispose();if(region!=IntPtr.Zero)DeleteObject(region);
            result.Stopped=preview==null||preview.HasExited;
            if(result.Stopped&&logs!=null&&logs.Wait(2000)) {
                string text=logs.Result??"";File.WriteAllText(Path.Combine(directory,result.Name+".log"),text,Encoding.UTF8);
                var m=Regex.Matches(text,@"\[glass\] requested=\w+ selected=(\w+)");
                if(m.Count>0)result.Selected=m[m.Count-1].Groups[1].Value;
                result.HostReported=text.Contains("[glass-host] native brush attached");
            }
            if(result.Stopped&&output!=null)output.Wait(2000);if(preview!=null)preview.Dispose();
        }return result;
    }
    public static Report Run(string exe,string directory) {
        Check(IntPtr.Size==8&&File.Exists(exe),"64-bit PowerShell and a built GlassPreview.exe are required.");
        var existing=Process.GetProcessesByName("GlassPreview");int count=existing.Length;foreach(var p in existing)p.Dispose();
        Check(count==0,"Close existing test previews first; none was stopped.");Check(!Directory.Exists(directory),"Refusing to overwrite output directory.");
        var report=new Report{Directory=directory};var cases=new List<Case>();IntPtr old=IntPtr.Zero,focus=GetForegroundWindow();
        try {
            Directory.CreateDirectory(directory);old=SetThreadDpiAwarenessContext(new IntPtr(-4));Check(old!=IntPtr.Zero,"Cannot enter physical-pixel context.");
            foreach(string theme in new string[]{"light","dark"})foreach(string backend in new string[]{"native","solid"}) {
                var c=RunCase(exe,directory,backend,theme);cases.Add(c);Check(c.Error==null&&c.Stopped,c.Error??"Case did not stop.");
            }report.Completed=true;
        }catch(Exception e){report.Error=e.Message;}
        finally {if(old!=IntPtr.Zero)SetThreadDpiAwarenessContext(old);report.FocusPreserved=focus==GetForegroundWindow();report.Cases=cases.ToArray();}
        return report;
    }
}
'@
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $env:TEMP ('doubao-native-response-' + [Guid]::NewGuid().ToString('N'))
}
$exe = Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
$result = [NativeResponseProbe14]::Run($exe,$OutputDirectory)
$result.ExecutableSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$json = $result | ConvertTo-Json -Depth 5 -Compress
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),$json)
}
Write-Output $json
# Images and full logs stay local; no Base64 or uploads.
if (-not $result.Completed) { throw ('Backdrop response probe failed: ' + $result.Error) }
$global:LASTEXITCODE = 0
