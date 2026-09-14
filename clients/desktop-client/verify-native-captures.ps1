# Cross-check native-only source modes with GDI and DXGI Desktop Duplication.
# Diagnostic only: no product deployment, foreign repaint, voice recording, or upload.
# -Source target-compare alternates the SAME HostBackdrop on lower/upper target slots.
# This is NOT WS_EX_TOPMOST: both native HWNDs retain identical topmost styles.
# -Source control alternates host/none. none has no compositor/brush/target at all.
# -Placement compare preserves the old topmost/restack fixture and adds ordinary
# non-topmost Show() without touching the already-topmost host's Z order.
# Crops are bounded by the monitor work area in BOTH arms; ordinary paper must
# not be asked to cover the taskbar. Host geometry and interior probes do not move.
# UnfilteredInputMismatches tests raw transparency, NOT blur quality in native modes.
[CmdletBinding()]
param([string]$OutputDirectory, [string]$FfmpegPath, [switch]$CompileOnly,
    [ValidateSet('host','host-upper','visual-blur','none','compare','control','target-compare')][string]$Source = 'host',
    [ValidateSet('restack','normal','compare')][string]$Placement = 'restack')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('NativeCaptureAudit24' -as [type])) {
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
public static class NativeCaptureAudit24 {
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
    [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] static extern IntPtr GetWindowLongPtr(IntPtr h,int index);
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
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d,int x,int y,int w,int ht,IntPtr s,int sx,int sy,uint rop);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    public sealed class Sample {
        public string Name,Api; public int Paints,OutsideChanged,ProbeCount,UnfilteredInputMismatches;
        public bool BackgroundRowValid; public double StartMs,EndMs;
        public double[] LeftRgb,RightRgb;
    }
    public sealed class Trial {
        public int Repeat,PaintsWhenShowReturned; public string Source,Placement,Error,Log;
        public int[] OuterPixels,CropPixels;
        public bool HostStopped,RegionPreserved,PaperUnchangedDuringColdDda,PaperUnchangedDuringWarmDda;
        public bool SourceTopmost,HostTopmost,StackingConfigurationVerified;
        public bool? TargetLayerVerified;
        public double? GdiInitialToWarm,DdaFirstInitialToWarm,DdaLastInitialToWarm,GdiAfterDdaToWarm;
        public double? ColdDdaVsGdiBefore,ColdDdaVsGdiAfter,ColdGdiChangeAcrossDda,ColdDdaFirstToLast,WarmDdaVsGdi;
        public Sample[] Samples;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved; public string Error,Directory,ExecutableSha256,FfmpegSha256;
        public string Scope="Native-only diagnostic. target-compare uses the same HostBackdrop constructor with only the creation-time target-slot bool changed and read back; both HWNDs stay topmost. normal placement uses non-topmost paper Show only. Cold/repaint phases, probe regions and GDI/four-DXGI/GDI capture phases are unchanged. A normal initial sample is not a recovery. Raw input mismatches are not blur-quality metrics. No automatic product acceptance, OS-bug attribution, FPS claim or upload.";
        public Trial[] Trials;
    }
    sealed class Paper : Form {
        public int SplitScreenX,Paints;
        public Paper(bool topmost){FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;TopMost=topmost;StartPosition=FormStartPosition.Manual;AutoScaleMode=AutoScaleMode.None;DoubleBuffered=true;}
        protected override bool ShowWithoutActivation {get{return true;}}
        protected override CreateParams CreateParams {get{var p=base.CreateParams;p.ExStyle|=0x08000080;return p;}}
        public Color Expected(int x){return x<SplitScreenX?Color.FromArgb(220,48,64):Color.FromArgb(32,112,224);}
        protected override void OnPaint(PaintEventArgs e){Paints++;e.Graphics.Clear(Expected(Left));int s=Math.Max(0,Math.Min(ClientSize.Width,SplitScreenX-Left));
            using(var b=new SolidBrush(Expected(Left+s)))e.Graphics.FillRectangle(b,s,0,ClientSize.Width-s,ClientSize.Height);}
    }
    static void Check(bool ok,string m){if(!ok)throw new InvalidOperationException(m);}
    static uint Owner(IntPtr h){uint p;GetWindowThreadProcessId(h,out p);return p;}
    static bool Topmost(IntPtr h){return (GetWindowLongPtr(h,-20).ToInt64()&8)!=0;}
    static IntPtr Root(int x,int y){return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2);}
    static void Pump(int ms){var t=Stopwatch.StartNew();do{Application.DoEvents();System.Threading.Thread.Sleep(10);}while(t.ElapsedMilliseconds<ms);}
    static void ClearOfOtherHosts(){
        foreach(string name in new string[]{"GlassPreview","GlassNativeMinimal"})foreach(var p in Process.GetProcessesByName(name)){
            using(p){Check(p.HasExited,"Existing diagnostic process: "+name+" PID="+p.Id+"; none was stopped.");}}
    }
    static IntPtr Find(uint pid){IntPtr found=IntPtr.Zero;int n=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused){if(Owner(h)==pid&&IsWindowVisible(h)){
            var t=new StringBuilder(256);GetWindowText(h,t,t.Capacity);if(t.ToString()=="Doubao Glass Preview - no microphone"){found=h;n++;}}return true;};
        Check(EnumWindows(cb,IntPtr.Zero)&&n<=1,"Cannot uniquely find test host.");return found;}
    static void StopHost(Process p,IntPtr h){if(p==null||p.HasExited)return;
        if(h!=IntPtr.Zero&&Owner(h)==(uint)p.Id)PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)){p.Kill();Check(p.WaitForExit(5000),"Test host did not exit.");}}
    static void LayerCheck(Rectangle crop,IntPtr host,Paper paper){
        Check(IsWindowVisible(host)&&Root(crop.Left+crop.Width/2,crop.Top+crop.Height/2)==host,"Capture center is not the test host.");
        Check(Root(crop.Left+3,crop.Top+3)==paper.Handle&&Root(crop.Right-4,crop.Bottom-4)==paper.Handle,"Synthetic capture margins are covered.");
        Check(Topmost(host)&&Topmost(paper.Handle)==paper.TopMost,"Actual topmost configuration changed.");}
    static Bitmap Gdi(Rectangle crop,IntPtr host,Paper paper){LayerCheck(crop,host,paper);
        var b=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try{using(var g=Graphics.FromImage(b)){IntPtr src=GetDC(IntPtr.Zero),dst=IntPtr.Zero;
            try{Check(src!=IntPtr.Zero,"No screen DC.");dst=g.GetHdc();Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"GDI capture failed.");}
            finally{if(dst!=IntPtr.Zero)g.ReleaseHdc(dst);if(src!=IntPtr.Zero)ReleaseDC(IntPtr.Zero,src);}}
            LayerCheck(crop,host,paper);return b;}catch{b.Dispose();throw;}}
    static Bitmap[] Dda(string ffmpeg,string prefix,Rectangle crop,IntPtr host,Paper paper){
        LayerCheck(crop,host,paper);
        Check(Screen.AllScreens.Length==1&&Screen.PrimaryScreen.Bounds.Location==Point.Empty,"DXGI probe requires a single display at (0,0).");
        Check(crop.Left>=0&&crop.Top>=0&&Screen.PrimaryScreen.Bounds.Contains(crop),"Invalid DXGI crop.");
        string filter="ddagrab=output_idx=0:draw_mouse=0:framerate=10:video_size="+crop.Width+"x"+crop.Height+":offset_x="+crop.Left+":offset_y="+crop.Top+",hwdownload,format=bgra";
        string args="-hide_banner -nostdin -nostats -loglevel error -filter_complex \""+filter+"\" -an -frames:v 4 -c:v png -threads:v 1 -start_number 0 -y \""+prefix+"-%d.png\"";
        Process p=null;System.Threading.Tasks.Task<string> err=null,output=null;
        try{p=Process.Start(new ProcessStartInfo(ffmpeg,args){UseShellExecute=false,CreateNoWindow=true,RedirectStandardError=true,RedirectStandardOutput=true});
            Check(p!=null,"Cannot start FFmpeg.");err=p.StandardError.ReadToEndAsync();output=p.StandardOutput.ReadToEndAsync();
            var timer=Stopwatch.StartNew();while(!p.HasExited&&timer.ElapsedMilliseconds<12000){LayerCheck(crop,host,paper);Pump(10);}
            Check(p.HasExited,"FFmpeg exceeded 12 seconds; no GDI fallback will substitute for it.");
            p.WaitForExit();Check(p.ExitCode==0,"FFmpeg capture failed; inspect the saved ffmpeg log.");LayerCheck(crop,host,paper);
        }finally{if(p!=null){try{if(!p.HasExited){p.Kill();Check(p.WaitForExit(5000),"Owned FFmpeg did not exit.");}}
                finally{if(err!=null&&err.Wait(2000))File.WriteAllText(prefix+"-ffmpeg.log",err.Result??"",Encoding.UTF8);
                    if(output!=null)output.Wait(2000);p.Dispose();}}}
        var frames=new List<Bitmap>();
        try{for(int i=0;i<4;i++){string file=prefix+"-"+i+".png";Check(File.Exists(file),"Missing DXGI frame: "+i);
                using(var raw=new Bitmap(file)){Check(raw.Size==crop.Size,"DXGI frame size mismatch.");frames.Add(new Bitmap(raw));}}
            return frames.ToArray();}catch{foreach(var b in frames)b.Dispose();throw;}
    }
    static double Delta(Bitmap a,Bitmap b,List<Point> points){double v=0;foreach(var p in points){var x=a.GetPixel(p.X,p.Y);var y=b.GetPixel(p.X,p.Y);
        v+=Math.Abs(x.R-y.R)+Math.Abs(x.G-y.G)+Math.Abs(x.B-y.B);}return Math.Round(v/(3*points.Count),4);}
    static double[] Mean(Bitmap b,List<Point> points){double[] v=new double[3];foreach(var p in points){var c=b.GetPixel(p.X,p.Y);v[0]+=c.R;v[1]+=c.G;v[2]+=c.B;}
        for(int k=0;k<3;k++)v[k]=Math.Round(v[k]/points.Count,3);return v;}
    static bool Outside(IntPtr r,int x,int y){return !PtInRegion(r,x,y)&&!PtInRegion(r,x-3,y)&&!PtInRegion(r,x+3,y)&&!PtInRegion(r,x,y-3)&&!PtInRegion(r,x,y+3);}
    static Sample Note(Bitmap b,string name,string api,double start,double end,Paper paper,Rectangle crop,Rectangle outer,IntPtr region,List<Point> left,List<Point> right){
        bool valid=true;for(int x=4;x<b.Width-4;x++){var a=b.GetPixel(x,4);var e=paper.Expected(crop.Left+x);
            if(Math.Max(Math.Abs(a.R-e.R),Math.Max(Math.Abs(a.G-e.G),Math.Abs(a.B-e.B)))>2)valid=false;}
        int bad=0;for(int y=0;y<b.Height;y++)for(int x=0;x<b.Width;x++)if(Outside(region,x+crop.Left-outer.Left,y+crop.Top-outer.Top)){
            var a=b.GetPixel(x,y);var e=paper.Expected(crop.Left+x);if(Math.Max(Math.Abs(a.R-e.R),Math.Max(Math.Abs(a.G-e.G),Math.Abs(a.B-e.B)))>4)bad++;}
        int rawBad=0;
        foreach(var patch in new List<Point>[]{left,right})foreach(var p in patch){
            var a=b.GetPixel(p.X,p.Y);var e=paper.Expected(crop.Left+p.X);
            if(Math.Max(Math.Abs(a.R-e.R),Math.Max(Math.Abs(a.G-e.G),Math.Abs(a.B-e.B)))>2)rawBad++;
        }
        return new Sample{Name=name,Api=api,StartMs=start,EndMs=end,Paints=paper.Paints,BackgroundRowValid=valid,OutsideChanged=bad,
            ProbeCount=left.Count+right.Count,UnfilteredInputMismatches=rawBad,LeftRgb=Mean(b,left),RightRgb=Mean(b,right)};
    }
    static Trial RunTrial(string exe,string ffmpeg,string dir,int repeat,string source,string placement){
        var result=new Trial{Repeat=repeat,Source=source,Placement=placement};var notes=new List<Sample>();var images=new List<Bitmap>();
        Process host=null;Paper paper=null;IntPtr h=IntPtr.Zero,region=IntPtr.Zero;
        System.Threading.Tasks.Task<string> logs=null,stdout=null;var clock=Stopwatch.StartNew();
        try{var si=new ProcessStartInfo(exe,"--glass-backend=native --theme=dark --content=optimizing --seconds=60 --backdrop-source="+source){
                UseShellExecute=false,CreateNoWindow=true,WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true};
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");host=Process.Start(si);Check(host!=null,"Cannot start minimal host.");
            logs=host.StandardError.ReadToEndAsync();stdout=host.StandardOutput.ReadToEndAsync();
            var timer=Stopwatch.StartNew();while(timer.ElapsedMilliseconds<12000&&!host.HasExited){h=Find((uint)host.Id);if(h!=IntPtr.Zero)break;Pump(20);}
            Check(h!=IntPtr.Zero&&!host.HasExited,"Minimal host did not appear.");Pump(1500);
            R r;Check(GetWindowRect(h,out r),"Cannot read test host geometry.");var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected host size.");
            var screen=Screen.PrimaryScreen.Bounds;
            // Both arms use the same work-area crop, not the taskbar or a smaller
            // hand-picked success region. Keep the entire HWND and interior probes.
            var crop=Rectangle.Intersect(Rectangle.Inflate(outer,24,24),Screen.PrimaryScreen.WorkingArea);
            Check(screen.Contains(crop)&&crop.Contains(outer)&&outer.Left-crop.Left>=4&&outer.Top-crop.Top>=4
                &&crop.Right-outer.Right>=4&&crop.Bottom-outer.Bottom>=4,"Insufficient work-area margin; host was not moved.");
            result.OuterPixels=new int[]{outer.Left,outer.Top,outer.Width,outer.Height};
            result.CropPixels=new int[]{crop.Left,crop.Top,crop.Width,crop.Height};
            region=CreateRectRgn(0,0,0,0);Check(region!=IntPtr.Zero&&GetWindowRgn(h,region)>1,"No capsule region.");
            R box;Check(GetRgnBox(region,out box)>1,"No capsule bounds.");
            double cx=(box.L+box.Right)/2.0,cy=(box.T+box.Bottom)/2.0,w=box.Right-box.L,ht=box.Bottom-box.T;
            var left=new List<Point>();var right=new List<Point>();var points=new List<Point>();
            for(int y=box.T;y<box.Bottom;y++)for(int x=box.L;x<box.Right;x++){
                double dx=Math.Abs(x+.5-cx),dy=Math.Abs(y+.5-cy);if(dx>w*.34&&dx<w*.42&&dy<ht*.16&&PtInRegion(region,x,y)){
                    var p=new Point(x+outer.Left-crop.Left,y+outer.Top-crop.Top);points.Add(p);if(x+.5<cx)left.Add(p);else right.Add(p);}}
            Check(left.Count>=20&&right.Count>=20,"Insufficient probes.");Pump(400);
            Check(Topmost(h),"Host is not topmost before source creation.");
            paper=new Paper(placement=="restack");paper.SplitScreenX=outer.Left+(int)cx;
            int pw=Math.Min(800,screen.Width),ph=Math.Min(340,screen.Height);
            paper.Bounds=new Rectangle(screen.Left+(screen.Width-pw)/2,screen.Bottom-ph,pw,ph);
            Check(paper.Bounds.Contains(crop),"Paper does not cover crop.");
            paper.Show();result.PaintsWhenShowReturned=paper.Paints;
            if(placement=="restack"){
                // Preserve the old fixture exactly: topmost Show(), then reorder.
                Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack host.");
                Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack paper.");
            }
            // normal: NO host raise, source restack, explicit repaint or activation.
            // Obstruction by another window is a test error, never corrected silently.
            Pump(1600);
            result.SourceTopmost=Topmost(paper.Handle);result.HostTopmost=Topmost(h);
            result.StackingConfigurationVerified=result.HostTopmost&&result.SourceTopmost==(placement=="restack");
            Check(result.StackingConfigurationVerified,"Requested source/host topmost styles were not applied.");
            for(int stage=0;stage<2;stage++){
                if(stage==1){paper.Refresh();Pump(1600);}
                string label=stage==0?"cold":"warm";string prefix=Path.Combine(dir,"r"+repeat+"-"+label);
                double t0=clock.Elapsed.TotalMilliseconds;var before=Gdi(crop,h,paper);double t1=clock.Elapsed.TotalMilliseconds;
                images.Add(before);before.Save(prefix+"-gdi-before.png",ImageFormat.Png);int paintsBefore=paper.Paints;
                notes.Add(Note(before,label+"-gdi-before","GDI",t0,t1,paper,crop,outer,region,left,right));
                t0=clock.Elapsed.TotalMilliseconds;var frames=Dda(ffmpeg,prefix+"-dda",crop,h,paper);t1=clock.Elapsed.TotalMilliseconds;
                images.AddRange(frames);
                for(int i=0;i<frames.Length;i++)notes.Add(Note(frames[i],label+"-dda-"+i,"DXGI interval (not present timestamp)",t0,t1,paper,crop,outer,region,left,right));
                t0=clock.Elapsed.TotalMilliseconds;var after=Gdi(crop,h,paper);t1=clock.Elapsed.TotalMilliseconds;
                images.Add(after);after.Save(prefix+"-gdi-after.png",ImageFormat.Png);
                notes.Add(Note(after,label+"-gdi-after","GDI",t0,t1,paper,crop,outer,region,left,right));
                if(stage==0)result.PaperUnchangedDuringColdDda=paper.Paints==paintsBefore;else result.PaperUnchangedDuringWarmDda=paper.Paints==paintsBefore;
                R now;Check(GetWindowRect(h,out now),"Cannot reread host geometry.");
                Check(now.L==r.L&&now.T==r.T&&now.Right==r.Right&&now.Bottom==r.Bottom,"Host moved during capture.");
                IntPtr actual=CreateRectRgn(0,0,0,0);Check(actual!=IntPtr.Zero,"Cannot allocate region check.");
                try{result.RegionPreserved=GetWindowRgn(h,actual)>1&&EqualRgn(region,actual);}finally{DeleteObject(actual);}
                Check(result.RegionPreserved,"Host region changed.");foreach(var note in notes)Check(note.BackgroundRowValid,"Capture background is not the expected RGB; color/mapping failure, not blur evidence.");
            }
            result.GdiInitialToWarm=Delta(images[0],images[6],points);
            result.DdaFirstInitialToWarm=Delta(images[1],images[7],points);result.DdaLastInitialToWarm=Delta(images[4],images[10],points);
            result.GdiAfterDdaToWarm=Delta(images[5],images[11],points);
            result.ColdDdaVsGdiBefore=Delta(images[4],images[0],points);result.ColdDdaVsGdiAfter=Delta(images[4],images[5],points);
            result.ColdGdiChangeAcrossDda=Delta(images[0],images[5],points);result.ColdDdaFirstToLast=Delta(images[1],images[4],points);
            result.WarmDdaVsGdi=Delta(images[10],images[11],points);
        }catch(Exception e){result.Error=e.Message;}
        finally{try{StopHost(host,h);}catch(Exception e){result.Error=(result.Error??"")+" Cleanup: "+e.Message;}
            if(paper!=null){paper.Close();paper.Dispose();Application.DoEvents();}foreach(var b in images)b.Dispose();if(region!=IntPtr.Zero)DeleteObject(region);
            result.HostStopped=host==null||host.HasExited;result.Samples=notes.ToArray();
            try{if(result.HostStopped&&logs!=null&&logs.Wait(2000)){string text=logs.Result??"";File.WriteAllText(Path.Combine(dir,"r"+repeat+"-host.log"),text,Encoding.UTF8);
                    result.Log=text.Length>1000?text.Substring(text.Length-1000):text;
                    bool hostMode=source=="host"||source=="host-upper";
                    string marker=hostMode?"[glass-host] native brush attached":source=="none"?"[glass-clear] compositor=absent;":"[glass-visual-blur] attached;";
                    string selected=source=="none"?"Transparent":"Native";
                    if(!text.Contains("[glass-minimal] selected="+selected+";")||!text.Contains("source="+source+";")||!text.Contains(marker)||text.Contains("[glass-minimal] failed:"))result.Error=(result.Error??"")+" Minimal source identity/runtime validation failed.";
                    if(hostMode){
                        string layer=source=="host-upper"?"upper":"lower";
                        result.TargetLayerVerified=text.Contains("[glass-host-target] requested="+layer+"; actual="+layer+"; readback=verified");
                        if(result.TargetLayerVerified!=true)result.Error=(result.Error??"")+" Target layer readback was not verified.";
                    }
                    if(source=="none"&&(text.Contains("[glass-host] native brush attached")||text.Contains("[glass-visual-blur] attached;")))result.Error=(result.Error??"")+" Zero-effect control unexpectedly initialized a brush.";}
                else result.Error=(result.Error??"")+" Host log unavailable.";if(result.HostStopped&&stdout!=null)stdout.Wait(2000);
            }catch(Exception e){result.Error=(result.Error??"")+" Log: "+e.Message;}if(host!=null)host.Dispose();}
        return result;
    }
    public static Report Run(string exe,string ffmpeg,string dir,string source,string placement){
        Check(source=="host"||source=="host-upper"||source=="visual-blur"||source=="none"||source=="compare"||source=="control"||source=="target-compare","Invalid source choice.");
        Check(placement=="restack"||placement=="normal"||placement=="compare","Invalid source-placement choice.");
        Check(placement!="compare"||(source!="compare"&&source!="control"&&source!="target-compare"),"Select one backdrop source when comparing source placement.");
        Check(IntPtr.Size==8&&File.Exists(exe)&&File.Exists(ffmpeg),"64-bit PowerShell, minimal host and FFmpeg are required.");
        ClearOfOtherHosts();Check(!Directory.Exists(dir),"Refusing to overwrite output directory.");
        var report=new Report{Directory=dir};var trials=new List<Trial>();IntPtr old=IntPtr.Zero,focus=GetForegroundWindow();
        try{Directory.CreateDirectory(dir);old=SetThreadDpiAwarenessContext(new IntPtr(-4));Check(old!=IntPtr.Zero,"Cannot enter physical-pixel context.");
            Check(Screen.AllScreens.Length==1&&Screen.PrimaryScreen.Bounds.Location==Point.Empty,"Single-display diagnostic only; no capture was started.");
            for(int i=1;i<=3;i++){
                string[] modes;
                if(source=="compare")modes=i%2==1?new string[]{"host","visual-blur"}:new string[]{"visual-blur","host"};
                else if(source=="control")modes=i%2==1?new string[]{"host","none"}:new string[]{"none","host"};
                else if(source=="target-compare")modes=i%2==1?new string[]{"host","host-upper"}:new string[]{"host-upper","host"};
                else modes=new string[]{source};
                string[] placements=placement=="compare"?(i%2==1?new string[]{"restack","normal"}:new string[]{"normal","restack"}):new string[]{placement};
                foreach(string mode in modes)foreach(string position in placements){
                    string sub=Path.Combine(dir,mode+"-"+position+"-r"+i);Directory.CreateDirectory(sub);
                    var trial=RunTrial(exe,ffmpeg,sub,i,mode,position);trials.Add(trial);
                    Check(trial.Error==null&&trial.HostStopped,trial.Error??"Test cleanup failed.");
                }
            }report.Completed=true;
        }catch(Exception e){report.Error=e.Message;}
        finally{if(old!=IntPtr.Zero)SetThreadDpiAwarenessContext(old);report.FocusPreserved=focus==GetForegroundWindow();report.Trials=trials.ToArray();}
        return report;
    }
}
'@
}
if ($CompileOnly) { Write-Output 'Cross-capture C# compiled; no process or capture was started.'; $global:LASTEXITCODE=0; return }
if ([string]::IsNullOrWhiteSpace($FfmpegPath)) {
    $FfmpegPath = (Get-Command ffmpeg.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
}
$exe=Join-Path $PSScriptRoot 'target\release\examples\GlassNativeMinimal.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Build GlassNativeMinimal before this diagnostic.' }
$hash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$ffhash=(Get-FileHash -LiteralPath $FfmpegPath -Algorithm SHA256).Hash
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory=Join-Path $env:TEMP ('doubao-capture-paths-'+[Guid]::NewGuid().ToString('N')) }
$result=[NativeCaptureAudit24]::Run($exe,$FfmpegPath,$OutputDirectory,$Source,$Placement)
$result.ExecutableSha256=$hash; $result.FfmpegSha256=$ffhash
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) { [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),($result|ConvertTo-Json -Depth 8 -Compress)) }
# Bound the paired six-trial output. Full RGB and capture intervals stay local.
$trials=@($result.Trials | ForEach-Object {
    $t=$_; $s=@($t.Samples); $warm=@($s|Where-Object {$_.Name -eq 'warm-gdi-after'}); $cold=@($s|Where-Object {$_.Name -eq 'cold-gdi-before'})
    $wl=@(); $wr=@(); $order=$null; $coldBad=$null; $warmBad=$null; $probes=$null
    if ($cold.Count -eq 1) { $coldBad=$cold[0].UnfilteredInputMismatches; $probes=$cold[0].ProbeCount }
    if ($warm.Count -eq 1) {
        $wl=@($warm[0].LeftRgb); $wr=@($warm[0].RightRgb); $warmBad=$warm[0].UnfilteredInputMismatches
        if ($wl.Count -eq 3 -and $wr.Count -eq 3) { $order=$wl[0] -gt $wl[2] -and $wr[2] -gt $wr[0] }
    }
    [ordered]@{Source=$t.Source;Placement=$t.Placement;Repeat=$t.Repeat;Error=$t.Error;Stopped=$t.HostStopped;
        TargetLayerVerified=$t.TargetLayerVerified;
        SourceTopmost=$t.SourceTopmost;HostTopmost=$t.HostTopmost;StackingVerified=$t.StackingConfigurationVerified;PaintsAtShowReturn=$t.PaintsWhenShowReturned;
        OuterPixels=$t.OuterPixels;CropPixels=$t.CropPixels;
        GdiInitialToWarm=$t.GdiInitialToWarm;DdaFirstInitialToWarm=$t.DdaFirstInitialToWarm;DdaLastInitialToWarm=$t.DdaLastInitialToWarm;
        ColdDdaVsGdi=$t.ColdDdaVsGdiBefore;ColdGdiChangeAcrossDda=$t.ColdGdiChangeAcrossDda;WarmDdaVsGdi=$t.WarmDdaVsGdi;
        PaperUnchangedCold=$t.PaperUnchangedDuringColdDda;RegionPreserved=$t.RegionPreserved;
        PaintCounts=@($s|ForEach-Object {$_.Paints});MaxOutsideChanged=($s|Measure-Object -Property OutsideChanged -Maximum).Maximum;
        AllBackgroundRowsValid=($s.Count -eq 12 -and @($s|Where-Object {-not $_.BackgroundRowValid}).Count -eq 0);
        ProbePixels=$probes;ColdUnfilteredMismatches=$coldBad;WarmUnfilteredMismatches=$warmBad;
        WarmLeftRgb=$wl;WarmRightRgb=$wr;WarmRedBlueOrder=$order}
})
[ordered]@{Completed=$result.Completed;Error=$result.Error;FocusPreserved=$result.FocusPreserved;Directory=$result.Directory;ExecutableSha256=$hash;FfmpegSha256=$ffhash;Scope=$result.Scope;Trials=$trials}|ConvertTo-Json -Depth 5 -Compress
if (-not $result.Completed) { throw ('Cross-capture test failed: '+$result.Error) }
$global:LASTEXITCODE=0
