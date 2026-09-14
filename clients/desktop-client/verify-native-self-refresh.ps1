# Preview-only candidate recovery trial. The production client is never opened,
# stopped, subclassed or repainted. All candidate calls target our test HWND.
[CmdletBinding()]
param([string]$OutputDirectory, [switch]$CompileOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('NativeSelfRefresh17' -as [type])) {
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
public static class NativeSelfRefresh17 {
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
    [DllImport("gdi32.dll")] static extern bool EqualRgn(IntPtr a,IntPtr b);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(P p);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint flags);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h,IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d,int x,int y,int w,int h,IntPtr s,int sx,int sy,uint rop);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool RedrawWindow(IntPtr h,IntPtr rect,IntPtr region,uint flags);
    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(IntPtr h,int attribute,ref int value,int size);
    [DllImport("dwmapi.dll")] static extern int DwmFlush();
    public sealed class Case {
        public string Name,Selected="not-reported",Error,Log;
        public bool Stopped,RegionPreserved,InputsValidated;
        public int PaperPaintsBefore,PaperPaintsAfter,OutsideChanged;
        public double? BeforeToWarm,AfterToWarm,OwnChange,PositiveControlChange;
        public double CallMs;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved;
        public string Directory,Error,ExecutableSha256;
        public string Interpretation="Candidate recovery, not a product fix. Reset may show a transient frame. No automatic visual/FPS acceptance.";
        public Case[] Cases;
    }
    sealed class Paper : Form {
        public int SplitScreenX,PaintCount;
        public Paper() {
            FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;TopMost=true;
            StartPosition=FormStartPosition.Manual;AutoScaleMode=AutoScaleMode.None;DoubleBuffered=true;
        }
        protected override bool ShowWithoutActivation { get {return true;} }
        protected override CreateParams CreateParams {get {var p=base.CreateParams;p.ExStyle|=0x08000080;return p;}}
        public Color Expected(int x) {return x<SplitScreenX?Color.FromArgb(220,48,64):Color.FromArgb(32,112,224);}
        protected override void OnPaint(PaintEventArgs e) {
            ++PaintCount;
            e.Graphics.Clear(Expected(Left));
            int split=Math.Max(0,Math.Min(ClientSize.Width,SplitScreenX-Left));
            using(var b=new SolidBrush(Expected(Left+split)))e.Graphics.FillRectangle(b,split,0,ClientSize.Width-split,ClientSize.Height);
        }
    }
    static void Check(bool ok,string m){if(!ok)throw new InvalidOperationException(m);}
    static uint Owner(IntPtr h){uint p;GetWindowThreadProcessId(h,out p);return p;}
    static IntPtr Root(int x,int y){return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2);}
    static void Pump(int ms){var t=Stopwatch.StartNew();do{Application.DoEvents();System.Threading.Thread.Sleep(10);}while(t.ElapsedMilliseconds<ms);}
    static IntPtr Find(uint pid){
        IntPtr found=IntPtr.Zero;int n=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused){
            if(Owner(h)==pid&&IsWindowVisible(h)){var s=new StringBuilder(256);GetWindowText(h,s,s.Capacity);
                if(s.ToString()=="Doubao Glass Preview - no microphone"){found=h;n++;}}
            return true;};
        Check(EnumWindows(cb,IntPtr.Zero)&&n<=1,"Preview enumeration failed.");return found;
    }
    static void Stop(Process p,IntPtr h){
        if(p==null||p.HasExited)return;
        if(h!=IntPtr.Zero&&Owner(h)==(uint)p.Id)PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)){p.Kill();Check(p.WaitForExit(5000),"Test preview did not exit.");}
    }
    static Bitmap Shot(Rectangle crop,IntPtr expected,Paper paper,Point center,string path){
        Check(Root(center.X,center.Y)==expected&&Root(crop.Left+3,crop.Top+3)==paper.Handle,"Test area is covered.");
        var image=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try{
            using(var g=Graphics.FromImage(image)){
                IntPtr src=GetDC(IntPtr.Zero),dst=IntPtr.Zero;
                try{Check(src!=IntPtr.Zero,"No screen DC.");dst=g.GetHdc();Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"Capture failed.");}
                finally{if(dst!=IntPtr.Zero)g.ReleaseHdc(dst);if(src!=IntPtr.Zero)ReleaseDC(IntPtr.Zero,src);}}
            Check(Root(center.X,center.Y)==expected,"Capture layering changed.");
            for(int x=4;x<image.Width-4;x++){var a=image.GetPixel(x,4);var b=paper.Expected(crop.Left+x);
                Check(Math.Abs(a.R-b.R)<=2&&Math.Abs(a.G-b.G)<=2&&Math.Abs(a.B-b.B)<=2,"Background validation failed.");}
            image.Save(path,ImageFormat.Png);return image;
        }catch{image.Dispose();throw;}
    }
    static double Delta(Bitmap a,Bitmap b,List<Point> points){
        double sum=0;foreach(var p in points){var x=a.GetPixel(p.X,p.Y);var y=b.GetPixel(p.X,p.Y);
            sum+=Math.Abs(x.R-y.R)+Math.Abs(x.G-y.G)+Math.Abs(x.B-y.B);}
        return Math.Round(sum/(3*points.Count),4);
    }
    static void HostFlag(IntPtr h,int value){Check(DwmSetWindowAttribute(h,17,ref value,4)>=0,"Host backdrop opt-in request failed.");}
    static void Apply(string mode,IntPtr h,uint pid){
        Check(Owner(h)==pid&&IsWindowVisible(h),"Candidate target is not our preview.");
        if(mode=="none"){return;}
        if(mode=="own-redraw"){
            // Own HWND only. No RDW_ALLCHILDREN, no desktop/foreign invalidation.
            Check(RedrawWindow(h,IntPtr.Zero,IntPtr.Zero,0x401),"Own redraw request failed.");return;}
        if(mode=="host-reassert"){HostFlag(h,1);return;}
        Check(mode=="host-reset","Unexpected candidate.");
        HostFlag(h,0);
        try{Check(DwmFlush()>=0,"DWM flush failed.");Pump(80);}
        finally{if(Owner(h)==pid)HostFlag(h,1);}
        Check(DwmFlush()>=0,"DWM flush failed after restoring host backdrop.");
    }
    static Case RunCase(string exe,string directory,string theme,string mode){
        var result=new Case{Name=theme+"-"+mode};
        Process preview=null;Paper paper=null;IntPtr h=IntPtr.Zero,region=IntPtr.Zero;
        var images=new List<Bitmap>();System.Threading.Tasks.Task<string> log=null,output=null;
        try{
            var si=new ProcessStartInfo(exe,"--glass-backend=native --theme="+theme+" --content=optimizing --seconds=60"){
                UseShellExecute=false,CreateNoWindow=true,WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true};
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");preview=Process.Start(si);Check(preview!=null,"Cannot start preview.");
            log=preview.StandardError.ReadToEndAsync();output=preview.StandardOutput.ReadToEndAsync();
            var timer=Stopwatch.StartNew();while(timer.ElapsedMilliseconds<12000&&!preview.HasExited){h=Find((uint)preview.Id);if(h!=IntPtr.Zero)break;Pump(30);}
            Check(h!=IntPtr.Zero&&!preview.HasExited,"Preview did not appear.");Pump(1500);
            R r;Check(GetWindowRect(h,out r),"Cannot read geometry.");var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected geometry.");
            var screen=Screen.FromHandle(h).Bounds;var crop=Rectangle.Inflate(outer,24,24);Check(screen.Contains(crop),"Insufficient crop margin.");
            region=CreateRectRgn(0,0,0,0);Check(region!=IntPtr.Zero&&GetWindowRgn(h,region)>1,"No capsule region.");
            R box;Check(GetRgnBox(region,out box)>1,"Cannot read capsule bounds.");
            double cx=(box.L+box.Right)/2.0,cy=(box.T+box.Bottom)/2.0,w=box.Right-box.L,ht=box.Bottom-box.T;
            var points=new List<Point>();for(int y=box.T;y<box.Bottom;y++)for(int x=box.L;x<box.Right;x++){
                double dx=Math.Abs(x+.5-cx),dy=Math.Abs(y+.5-cy);
                if(dx>w*.34&&dx<w*.42&&dy<ht*.16&&PtInRegion(region,x,y))points.Add(new Point(x+outer.Left-crop.Left,y+outer.Top-crop.Top));}
            Check(points.Count>=40,"Insufficient interior probes.");var center=new Point(outer.Left+(int)cx,outer.Top+(int)cy);
            paper=new Paper();paper.SplitScreenX=center.X;paper.Bounds=Rectangle.Intersect(Rectangle.Inflate(crop,220,220),screen);paper.Show();
            Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack preview.");
            Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack paper.");
            // Preserve the failing order: first show behind the existing preview;
            // DO NOT repaint the paper before the candidate's post sample.
            Pump(1200);
            images.Add(Shot(crop,h,paper,center,Path.Combine(directory,result.Name+"-before.png")));
            result.PaperPaintsBefore=paper.PaintCount;
            timer.Restart();Apply(mode,h,(uint)preview.Id);result.CallMs=Math.Round(timer.Elapsed.TotalMilliseconds,3);
            Pump(1200);
            R now;Check(!preview.HasExited&&GetWindowRect(h,out now)&&now.L==r.L&&now.T==r.T&&now.Right==r.Right&&now.Bottom==r.Bottom,"Preview geometry changed.");
            images.Add(Shot(crop,h,paper,center,Path.Combine(directory,result.Name+"-after-own.png")));
            result.PaperPaintsAfter=paper.PaintCount;
            IntPtr actual=CreateRectRgn(0,0,0,0);Check(actual!=IntPtr.Zero,"Cannot allocate region check.");
            try{result.RegionPreserved=GetWindowRgn(h,actual)>1&&EqualRgn(region,actual);}finally{DeleteObject(actual);}
            Check(result.RegionPreserved,"Own refresh changed the capsule region.");
            // Positive control belongs to the TEST HARNESS ONLY, not a proposed
            // product remedy. It also establishes the intended final RGB values.
            paper.Refresh();Pump(1200);
            images.Add(Shot(crop,h,paper,center,Path.Combine(directory,result.Name+"-warm.png")));
            result.BeforeToWarm=Delta(images[0],images[2],points);result.AfterToWarm=Delta(images[1],images[2],points);
            result.OwnChange=Delta(images[0],images[1],points);result.PositiveControlChange=result.AfterToWarm;
            Stop(preview,h);Pump(300);
            using(var baseline=Shot(crop,paper.Handle,paper,center,Path.Combine(directory,result.Name+"-background.png"))){
                for(int y=0;y<baseline.Height;y++)for(int x=0;x<baseline.Width;x++){
                    int rx=x+crop.Left-outer.Left,ry=y+crop.Top-outer.Top;
                    if(PtInRegion(region,rx,ry)||PtInRegion(region,rx-3,ry)||PtInRegion(region,rx+3,ry)||PtInRegion(region,rx,ry-3)||PtInRegion(region,rx,ry+3))continue;
                    var a=images[1].GetPixel(x,y);var b=baseline.GetPixel(x,y);
                    if(Math.Max(Math.Abs(a.R-b.R),Math.Max(Math.Abs(a.G-b.G),Math.Abs(a.B-b.B)))>4)result.OutsideChanged++;
                }}
            result.InputsValidated=true;
        }catch(Exception e){result.Error=e.Message;}
        finally{
            try{Stop(preview,h);}catch(Exception e){result.Error=(result.Error??"")+" Cleanup: "+e.Message;}
            if(paper!=null){paper.Close();paper.Dispose();Application.DoEvents();}
            foreach(var b in images)b.Dispose();if(region!=IntPtr.Zero)DeleteObject(region);
            result.Stopped=preview==null||preview.HasExited;
            try{if(result.Stopped&&log!=null&&log.Wait(2000)){
                string text=log.Result??"";File.WriteAllText(Path.Combine(directory,result.Name+".log"),text,Encoding.UTF8);
                var m=Regex.Matches(text,@"\[glass\] requested=\w+ selected=(\w+)");if(m.Count>0)result.Selected=m[m.Count-1].Groups[1].Value;
                result.Log=text.Length<=700?text:text.Substring(text.Length-700);}
                if(result.Stopped&&output!=null)output.Wait(2000);
            }catch(Exception e){result.Error=(result.Error??"")+" Log: "+e.Message;}
            if(preview!=null)preview.Dispose();
        }return result;
    }
    public static Report Run(string exe,string directory){
        Check(IntPtr.Size==8&&File.Exists(exe),"64-bit Windows PowerShell and GlassPreview.exe are required.");
        var previews=Process.GetProcessesByName("GlassPreview");int n=previews.Length;foreach(var p in previews)p.Dispose();
        Check(n==0,"An existing preview is running; no changes made.");Check(!Directory.Exists(directory),"Refusing to overwrite output directory.");
        var result=new Report{Directory=directory};var cases=new List<Case>();IntPtr old=IntPtr.Zero,focus=GetForegroundWindow();
        try{
            Directory.CreateDirectory(directory);old=SetThreadDpiAwarenessContext(new IntPtr(-4));Check(old!=IntPtr.Zero,"Cannot enter physical-pixel context.");
            foreach(string theme in new string[]{"light","dark"})foreach(string mode in new string[]{"none","own-redraw","host-reassert","host-reset"}){
                var c=RunCase(exe,directory,theme,mode);cases.Add(c);Check(c.Error==null&&c.Stopped&&c.InputsValidated&&c.Selected=="Native",c.Error??"Native test case not validated.");}
            result.Completed=true;
        }catch(Exception e){result.Error=e.Message;}
        finally{if(old!=IntPtr.Zero)SetThreadDpiAwarenessContext(old);result.FocusPreserved=focus==GetForegroundWindow();result.Cases=cases.ToArray();}
        return result;
    }
}
'@
}
if ($CompileOnly) { Write-Output 'Diagnostic C# compiled; no preview was started.'; $global:LASTEXITCODE=0; return }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory=Join-Path $env:TEMP ('doubao-self-refresh-'+[Guid]::NewGuid().ToString('N'))
}
$exe=Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
$result=[NativeSelfRefresh17]::Run($exe,$OutputDirectory)
$result.ExecutableSha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),($result|ConvertTo-Json -Depth 7 -Compress))
}
$cases=@($result.Cases | Select-Object Name,Selected,Error,Stopped,BeforeToWarm,AfterToWarm,OwnChange,PaperPaintsBefore,PaperPaintsAfter,OutsideChanged)
[ordered]@{Completed=$result.Completed;FocusPreserved=$result.FocusPreserved;Error=$result.Error;Directory=$result.Directory;ExecutableSha256=$result.ExecutableSha256;Cases=$cases}|ConvertTo-Json -Depth 5 -Compress
if (-not $result.Completed) { throw ('Candidate recovery test failed: '+$result.Error) }
$global:LASTEXITCODE=0
