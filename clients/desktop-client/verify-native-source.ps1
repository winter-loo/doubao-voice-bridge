# Inspect only synthetic test windows. No voice input, foreign-window repaint,
# desktop upload or production-process control. This is not a material fix.
[CmdletBinding()]
param([string]$OutputDirectory, [switch]$CompileOnly,
    [ValidateSet('none','brush','visual')][string]$Refresh = 'none')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('GlassSourceAudit18' -as [type])) {
Add-Type -ReferencedAssemblies System.Drawing,System.Windows.Forms -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;
using System.Diagnostics;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public static class GlassSourceAudit18 {
    [StructLayout(LayoutKind.Sequential)] struct R { public int L,T,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct P { public int X,Y; }
    [StructLayout(LayoutKind.Sequential)] struct ThumbnailProperties {
        public uint Flags; public R Destination,Source; public byte Opacity;
        public int Visible,ClientOnly;
    }
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
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dst,int x,int y,int w,int ht,IntPtr src,int sx,int sy,uint rop);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("dwmapi.dll")] static extern int DwmRegisterThumbnail(IntPtr dst,IntPtr src,out IntPtr thumb);
    [DllImport("dwmapi.dll")] static extern int DwmUpdateThumbnailProperties(IntPtr thumb,ref ThumbnailProperties p);
    [DllImport("dwmapi.dll")] static extern int DwmUnregisterThumbnail(IntPtr thumb);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr OpenEvent(uint access,bool inherit,string name);
    [DllImport("kernel32.dll")] static extern bool SetEvent(IntPtr h);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    public sealed class Observation {
        public string Phase; public int PaperPaints; public double SampleMs;
        public double[] GlassLeft,GlassRight,SourceLeft,SourceRight;
        public int? SourceMismatches,SourceProbes;
        public bool? SourceOuterRowValid;
    }
    public sealed class Case {
        public string Name,Error,Log,Refresh; public bool Stopped;
        public double? BeforeToWarm,ObserverToWarm,ObserverChange,AfterOwnToWarm,OwnChange;
        public bool? SourceMappingVerifiedAfterRepaint,PaperUnchangedDuringOwn;
        public bool RefreshAcknowledged,RegionPreserved;
        public int OutsideChangedDuringOwn;
        public Observation[] Observations;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved; public string Directory,Error,ExecutableSha256;
        public string Scope="Synthetic source via 1:1 DWM thumbnail; one optional resource replacement. API acknowledgement is not presentation timing. No automatic visual acceptance.";
        public Case[] Cases;
    }
    class NoFocusForm : Form {
        public NoFocusForm() {
            FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;TopMost=true;
            StartPosition=FormStartPosition.Manual;AutoScaleMode=AutoScaleMode.None;
        }
        protected override bool ShowWithoutActivation {get{return true;}}
        protected override CreateParams CreateParams {
            get{var p=base.CreateParams;p.ExStyle|=0x08000080;return p;}
        }
    }
    sealed class Paper : NoFocusForm {
        public int SplitScreenX,Paints;
        public Paper(){DoubleBuffered=true;}
        public Color Expected(int globalX){return globalX<SplitScreenX?Color.FromArgb(220,48,64):Color.FromArgb(32,112,224);}
        protected override void OnPaint(PaintEventArgs e){
            Paints++;e.Graphics.Clear(Expected(Left));
            int split=Math.Max(0,Math.Min(ClientSize.Width,SplitScreenX-Left));
            using(var b=new SolidBrush(Expected(Left+split)))e.Graphics.FillRectangle(b,split,0,ClientSize.Width-split,ClientSize.Height);
        }
    }
    static void Check(bool ok,string m){if(!ok)throw new InvalidOperationException(m);}
    static uint Owner(IntPtr h){uint p;GetWindowThreadProcessId(h,out p);return p;}
    static IntPtr Root(int x,int y){return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2);}
    static IntPtr Find(uint pid){
        IntPtr found=IntPtr.Zero;int n=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused){
            if(Owner(h)==pid&&IsWindowVisible(h)){var t=new StringBuilder(256);GetWindowText(h,t,t.Capacity);
                if(t.ToString()=="Doubao Glass Preview - no microphone"){found=h;n++;}}
            return true;};
        Check(EnumWindows(cb,IntPtr.Zero)&&n<=1,"Preview enumeration failed.");return found;
    }
    static void Pump(int ms){var t=Stopwatch.StartNew();do{Application.DoEvents();System.Threading.Thread.Sleep(10);}while(t.ElapsedMilliseconds<ms);}
    static void Stop(Process p,IntPtr h){
        if(p==null||p.HasExited)return;
        if(h!=IntPtr.Zero&&Owner(h)==(uint)p.Id)PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)){p.Kill();Check(p.WaitForExit(5000),"Test preview did not exit.");}
    }
    static void RequestRenewal(Process process,IntPtr hwnd){
        Check(!process.HasExited&&Owner(hwnd)==(uint)process.Id,"Preview identity changed before probe.");
        string prefix="Local\\DoubaoGlassProbe."+process.Id+".";
        IntPtr request=IntPtr.Zero,applied=IntPtr.Zero,failed=IntPtr.Zero;
        try{
            request=OpenEvent(2,false,prefix+"request");
            applied=OpenEvent(0x00100000,false,prefix+"applied");
            failed=OpenEvent(0x00100000,false,prefix+"failed");
            Check(request!=IntPtr.Zero&&applied!=IntPtr.Zero&&failed!=IntPtr.Zero,"Preview probe channel is unavailable; rebuild the opt-in preview.");
            Check(WaitForSingleObject(applied,0)==258&&WaitForSingleObject(failed,0)==258,"Probe was already consumed.");
            Check(SetEvent(request),"Cannot signal preview probe.");
            var timer=Stopwatch.StartNew();
            while(timer.ElapsedMilliseconds<3000&&!process.HasExited){
                uint ok=WaitForSingleObject(applied,0),bad=WaitForSingleObject(failed,0);
                Check(bad==258,"Preview resource replacement failed; inspect saved log.");
                if(ok==0)return;
                Check(ok==258,"Cannot wait for preview acknowledgement.");Pump(10);
            }
            throw new InvalidOperationException("Preview did not acknowledge the one-shot probe.");
        }finally{
            if(request!=IntPtr.Zero)CloseHandle(request);
            if(applied!=IntPtr.Zero)CloseHandle(applied);
            if(failed!=IntPtr.Zero)CloseHandle(failed);
        }
    }
    static Bitmap Shot(Rectangle crop,IntPtr centerOwner,IntPtr marginOwner){
        Check(Root(crop.Left+crop.Width/2,crop.Top+crop.Height/2)==centerOwner&&Root(crop.Left+3,crop.Top+3)==marginOwner,
            "Another window covers the synthetic capture area.");
        var image=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try{using(var g=Graphics.FromImage(image)){
            IntPtr src=GetDC(IntPtr.Zero),dst=IntPtr.Zero;
            try{Check(src!=IntPtr.Zero,"Screen DC unavailable.");dst=g.GetHdc();
                Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"Capture failed.");
            }finally{if(dst!=IntPtr.Zero)g.ReleaseHdc(dst);if(src!=IntPtr.Zero)ReleaseDC(IntPtr.Zero,src);}}
            Check(Root(crop.Left+crop.Width/2,crop.Top+crop.Height/2)==centerOwner,"Layering changed during capture.");return image;
        }catch{image.Dispose();throw;}
    }
    static bool Near(Color a,Color b){return Math.Abs(a.R-b.R)<=2&&Math.Abs(a.G-b.G)<=2&&Math.Abs(a.B-b.B)<=2;}
    static bool RowValid(Bitmap b,Rectangle source,Paper paper){
        for(int x=4;x<b.Width-4;x++)if(!Near(b.GetPixel(x,4),paper.Expected(source.Left+x)))return false;return true;
    }
    static double[] Mean(Bitmap b,List<Point> p){double[] v=new double[3];foreach(var q in p){var c=b.GetPixel(q.X,q.Y);v[0]+=c.R;v[1]+=c.G;v[2]+=c.B;}
        for(int k=0;k<3;k++)v[k]=Math.Round(v[k]/p.Count,3);return v;}
    static double Delta(Bitmap a,Bitmap b,List<Point> points){double v=0;foreach(var p in points){var x=a.GetPixel(p.X,p.Y);var y=b.GetPixel(p.X,p.Y);
        v+=Math.Abs(x.R-y.R)+Math.Abs(x.G-y.G)+Math.Abs(x.B-y.B);}return Math.Round(v/(3*points.Count),4);}
    static Case RunCase(string exe,string directory,int repeat,string refresh){
        var result=new Case{Name="native-dark-preview-first-"+refresh+"-r"+repeat,Refresh=refresh};
        Process preview=null;Paper paper=null;NoFocusForm observer=null;IntPtr h=IntPtr.Zero,region=IntPtr.Zero,thumb=IntPtr.Zero;
        var images=new List<Bitmap>();var notes=new List<Observation>();
        System.Threading.Tasks.Task<string> stderr=null,stdout=null;var clock=Stopwatch.StartNew();
        try{
            var screen=Screen.PrimaryScreen.Bounds;
            string arguments="--glass-backend=native --theme=dark --content=optimizing --seconds=60";
            if(refresh!="none")arguments+=" --glass-probe="+refresh;
            var si=new ProcessStartInfo(exe,arguments){
                UseShellExecute=false,CreateNoWindow=true,WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true};
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");
            preview=Process.Start(si);Check(preview!=null,"Cannot start test preview.");
            stderr=preview.StandardError.ReadToEndAsync();stdout=preview.StandardOutput.ReadToEndAsync();
            var wait=Stopwatch.StartNew();while(wait.ElapsedMilliseconds<12000&&!preview.HasExited){h=Find((uint)preview.Id);if(h!=IntPtr.Zero)break;Pump(20);}
            Check(h!=IntPtr.Zero&&!preview.HasExited,"Preview did not appear.");Pump(1500);
            R r;Check(GetWindowRect(h,out r),"Cannot read preview geometry.");var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected preview size.");
            var crop=Rectangle.Inflate(outer,24,24);Check(screen.Contains(crop),"Insufficient monitor margin.");
            region=CreateRectRgn(0,0,0,0);Check(region!=IntPtr.Zero&&GetWindowRgn(h,region)>1,"No capsule region.");
            R box;Check(GetRgnBox(region,out box)>1,"No capsule region bounds.");
            double cx=(box.L+box.Right)/2.0,cy=(box.T+box.Bottom)/2.0,w=box.Right-box.L,ht=box.Bottom-box.T;
            var left=new List<Point>();var right=new List<Point>();var points=new List<Point>();
            for(int y=box.T;y<box.Bottom;y++)for(int x=box.L;x<box.Right;x++){
                double dx=Math.Abs(x+.5-cx),dy=Math.Abs(y+.5-cy);
                if(dx>w*.34&&dx<w*.42&&dy<ht*.16&&PtInRegion(region,x,y)){
                    var p=new Point(x+outer.Left-crop.Left,y+outer.Top-crop.Top);points.Add(p);if(x+.5<cx)left.Add(p);else right.Add(p);}}
            Check(left.Count>=20&&right.Count>=20,"Insufficient interior probe pixels.");
            paper=new Paper();paper.SplitScreenX=outer.Left+(int)cx;
            int pw=Math.Min(800,screen.Width),ph=Math.Min(340,screen.Height);
            paper.Bounds=new Rectangle(screen.Left+(screen.Width-pw)/2,screen.Bottom-ph,pw,ph);
            Check(paper.Bounds.Contains(crop),"Synthetic paper does not cover preview.");
            paper.Show();Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack preview.");
            Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack paper.");
            // Preserve first paint; do not refresh the source before the controls.
            Pump(1600);
            Rectangle view=Rectangle.Empty;
            string[] phases={"before-observer","observer-800","observer-1600","after-own-resource","after-source-repaint"};
            for(int i=0;i<phases.Length;i++){
                if(i==1){
                    observer=new NoFocusForm();observer.BackColor=Color.Magenta;
                    view=new Rectangle(crop.Left,paper.Top-crop.Height-40,crop.Width,crop.Height);
                    Check(screen.Contains(view)&&!view.IntersectsWith(paper.Bounds),"No safe space for the source observer.");
                    observer.Bounds=view;observer.Show();observer.Refresh();
                    R pr;Check(GetWindowRect(paper.Handle,out pr),"Cannot read source window geometry.");
                    R vr;Check(GetWindowRect(observer.Handle,out vr),"Cannot read observer window geometry.");
                    Check(vr.Right-vr.L==crop.Width&&vr.Bottom-vr.T==crop.Height&&observer.ClientSize==crop.Size,"Observer is not 1:1 physical pixels.");
                    view=Rectangle.FromLTRB(vr.L,vr.T,vr.Right,vr.Bottom);
                    using(var self=Process.GetCurrentProcess())Check(Owner(paper.Handle)==(uint)self.Id&&Owner(observer.Handle)==(uint)self.Id,"Source ownership mismatch.");
                    Check(DwmRegisterThumbnail(observer.Handle,paper.Handle,out thumb)>=0&&thumb!=IntPtr.Zero,"Cannot register source thumbnail.");
                    var properties=new ThumbnailProperties{Flags=0x1F,Opacity=255,Visible=1,ClientOnly=0,
                        Destination=new R{L=0,T=0,Right=crop.Width,Bottom=crop.Height},
                        Source=new R{L=crop.Left-pr.L,T=crop.Top-pr.T,Right=crop.Right-pr.L,Bottom=crop.Bottom-pr.T}};
                    Check(DwmUpdateThumbnailProperties(thumb,ref properties)>=0,"Cannot display source thumbnail.");Pump(800);
                }else if(i==2){Pump(800);}
                else if(i==3){
                    if(refresh!="none"){RequestRenewal(preview,h);result.RefreshAcknowledged=true;}
                    // No source repaint, host opt-in toggle or window recreation.
                    Pump(1200);
                }else if(i==4){paper.Refresh();Pump(1600);}
                R now;Check(!preview.HasExited&&GetWindowRect(h,out now)&&now.L==r.L&&now.T==r.T&&now.Right==r.Right&&now.Bottom==r.Bottom,"Preview changed or exited.");
                var image=Shot(crop,h,paper.Handle);images.Add(image);Check(RowValid(image,crop,paper),"Visible source margin does not match the requested colors.");
                image.Save(Path.Combine(directory,result.Name+"-"+phases[i]+".png"),ImageFormat.Png);
                var note=new Observation{Phase=phases[i],SampleMs=Math.Round(clock.Elapsed.TotalMilliseconds,2),PaperPaints=paper.Paints,
                    GlassLeft=Mean(image,left),GlassRight=Mean(image,right)};
                if(i>0){using(var source=Shot(view,observer.Handle,observer.Handle)){
                    source.Save(Path.Combine(directory,result.Name+"-"+phases[i]+"-source.png"),ImageFormat.Png);
                    note.SourceLeft=Mean(source,left);note.SourceRight=Mean(source,right);int bad=0;
                    foreach(var p in points)if(!Near(source.GetPixel(p.X,p.Y),paper.Expected(crop.Left+p.X)))bad++;
                    note.SourceMismatches=bad;note.SourceProbes=points.Count;note.SourceOuterRowValid=RowValid(source,crop,paper);
                }}notes.Add(note);
            }
            result.BeforeToWarm=Delta(images[0],images[4],points);
            result.ObserverToWarm=Delta(images[2],images[4],points);
            result.ObserverChange=Delta(images[0],images[2],points);
            result.AfterOwnToWarm=Delta(images[3],images[4],points);
            result.OwnChange=Delta(images[2],images[3],points);
            result.PaperUnchangedDuringOwn=notes[2].PaperPaints==notes[3].PaperPaints;
            IntPtr actual=CreateRectRgn(0,0,0,0);Check(actual!=IntPtr.Zero,"Cannot allocate region check.");
            try{result.RegionPreserved=GetWindowRgn(h,actual)>1&&EqualRgn(region,actual);}finally{DeleteObject(actual);}
            Check(result.RegionPreserved,"Candidate changed the capsule window region.");
            for(int y=0;y<crop.Height;y++)for(int x=0;x<crop.Width;x++){
                int rx=x+crop.Left-outer.Left,ry=y+crop.Top-outer.Top;
                if(PtInRegion(region,rx,ry)||PtInRegion(region,rx-3,ry)||PtInRegion(region,rx+3,ry)||PtInRegion(region,rx,ry-3)||PtInRegion(region,rx,ry+3))continue;
                var a=images[2].GetPixel(x,y);var b=images[3].GetPixel(x,y);
                if(Math.Max(Math.Abs(a.R-b.R),Math.Max(Math.Abs(a.G-b.G),Math.Abs(a.B-b.B)))>4)result.OutsideChangedDuringOwn++;
            }
            var last=notes[notes.Count-1];result.SourceMappingVerifiedAfterRepaint=last.SourceOuterRowValid==true&&last.SourceMismatches==0;
        }catch(Exception e){result.Error=e.Message;}
        finally{
            if(thumb!=IntPtr.Zero&&DwmUnregisterThumbnail(thumb)<0)result.Error=(result.Error??"")+" Thumbnail cleanup failed.";
            if(observer!=null){observer.Close();observer.Dispose();}
            try{Stop(preview,h);}catch(Exception e){result.Error=(result.Error??"")+" Preview cleanup: "+e.Message;}
            if(paper!=null){paper.Close();paper.Dispose();}Application.DoEvents();
            foreach(var b in images)b.Dispose();if(region!=IntPtr.Zero)DeleteObject(region);
            result.Stopped=preview==null||preview.HasExited;result.Observations=notes.ToArray();
            try{if(result.Stopped&&stderr!=null&&stderr.Wait(2000)){
                string log=stderr.Result??"";File.WriteAllText(Path.Combine(directory,result.Name+".log"),log,Encoding.UTF8);
                result.Log=log.Length>1100?log.Substring(log.Length-1100):log;
                if(!log.Contains("selected=Native")||!log.Contains("[glass-host] native brush attached"))result.Error=(result.Error??"")+" Native host visual not confirmed.";
                if(refresh!="none"&&(!log.Contains("[glass-host-probe] mode=")||!log.Contains("applied=true")))result.Error=(result.Error??"")+" Native resource replacement was not reported.";
            }else{result.Error=(result.Error??"")+" Native startup log unavailable.";}
            if(result.Stopped&&stdout!=null)stdout.Wait(2000);
            }catch(Exception e){result.Error=(result.Error??"")+" Log: "+e.Message;}
            if(preview!=null)preview.Dispose();
        }return result;
    }
    public static Report Run(string exe,string directory,string refresh){
        Check(refresh=="none"||refresh=="brush"||refresh=="visual","Invalid refresh mode.");
        Check(IntPtr.Size==8&&File.Exists(exe),"64-bit PowerShell and built GlassPreview are required.");
        Check(Marshal.SizeOf(typeof(ThumbnailProperties))==48,"Unexpected thumbnail structure layout.");
        var existing=Process.GetProcessesByName("GlassPreview");int n=existing.Length;foreach(var p in existing)p.Dispose();
        Check(n==0,"An existing test preview is running; none was stopped.");Check(!Directory.Exists(directory),"Refusing to overwrite output directory.");
        var report=new Report{Directory=directory};var cases=new List<Case>();IntPtr old=IntPtr.Zero,focus=GetForegroundWindow();
        try{Directory.CreateDirectory(directory);old=SetThreadDpiAwarenessContext(new IntPtr(-4));Check(old!=IntPtr.Zero,"Cannot enter physical-pixel DPI context.");
            for(int i=1;i<=3;i++){var c=RunCase(exe,directory,i,refresh);cases.Add(c);Check(c.Error==null&&c.Stopped,c.Error??"Test cleanup failed.");}report.Completed=true;
        }catch(Exception e){report.Error=e.Message;}
        finally{if(old!=IntPtr.Zero)SetThreadDpiAwarenessContext(old);report.Cases=cases.ToArray();report.FocusPreserved=focus==GetForegroundWindow();}
        return report;
    }
}
'@
}
if ($CompileOnly) { Write-Output 'Source diagnostic C# compiled; no test windows started.'; $global:LASTEXITCODE=0; return }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory=Join-Path $env:TEMP ('doubao-source-audit-'+[Guid]::NewGuid().ToString('N'))
}
$exe=Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
$result=[GlassSourceAudit18]::Run($exe,$OutputDirectory,$Refresh)
$result.ExecutableSha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),($result|ConvertTo-Json -Depth 8 -Compress))
}
# Bound bridge output. Full RGB, API logs and images remain in result.json.
$cases=@($result.Cases|ForEach-Object {
    $c=$_
    $samples=@($c.Observations|Select-Object Phase,PaperPaints,SourceMismatches,SourceProbes,SourceOuterRowValid)
    [ordered]@{Name=$c.Name;Error=$c.Error;Stopped=$c.Stopped;BeforeToWarm=$c.BeforeToWarm;ObserverToWarm=$c.ObserverToWarm;ObserverChange=$c.ObserverChange;AfterOwnToWarm=$c.AfterOwnToWarm;OwnChange=$c.OwnChange;RefreshAcknowledged=$c.RefreshAcknowledged;PaperUnchangedDuringOwn=$c.PaperUnchangedDuringOwn;RegionPreserved=$c.RegionPreserved;OutsideChangedDuringOwn=$c.OutsideChangedDuringOwn;SourceMappingVerifiedAfterRepaint=$c.SourceMappingVerifiedAfterRepaint;Observations=$samples}
})
[ordered]@{Completed=$result.Completed;Error=$result.Error;FocusPreserved=$result.FocusPreserved;Directory=$result.Directory;ExecutableSha256=$result.ExecutableSha256;Scope=$result.Scope;Cases=$cases}|ConvertTo-Json -Depth 6 -Compress
if (-not $result.Completed) { throw ('Source diagnostic failed: '+$result.Error) }
$global:LASTEXITCODE=0
