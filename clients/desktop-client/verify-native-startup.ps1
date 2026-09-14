# Isolated visual diagnostic: startup/background insertion vs repeatable response.
# Only GlassPreview and synthetic test paper are started. No microphone, screen
# upload, production process control, global setting changes, or source edits.
[CmdletBinding()]
param([string]$OutputDirectory, [switch]$CompileOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('GlassStartupAudit16' -as [type])) {
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
public static class GlassStartupAudit16 {
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
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
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
    public sealed class Sample {
        public string Phase,File;
        public double RequestedMs,CopyStartedMs,CopyDoneMs,ProcessAgeMs;
        public bool InputValidated;
        public double[] LeftRgb,RightRgb;
    }
    public sealed class Case {
        public string Name,Order,Selected="not-reported",Error,Log;
        public bool Stopped,HostReported,AllInputsValidated;
        public int ProbePixels;
        public double? FirstA800To3200,SameARepaintChange,FirstAToWarmA;
        public double? WarmResponse,WarmReturnA,WarmReturnB,WarmGain,FineGain,FineSignedGain,FineReturn;
        public Sample[] Samples;
    }
    public sealed class Report {
        public bool Completed,FocusPreserved;
        public string Directory,Error,ExecutableSha256;
        public string Interpretation="Startup is retained separately from warm repeated A/B. Actual GDI capture timestamps, not present timing. No automatic visual acceptance.";
        public Case[] Cases;
    }
    sealed class Paper : Form {
        public int Pattern,SplitScreenX;
        public Paper() {
            FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;TopMost=true;
            StartPosition=FormStartPosition.Manual;AutoScaleMode=AutoScaleMode.None;DoubleBuffered=true;
        }
        protected override bool ShowWithoutActivation { get { return true; } }
        protected override CreateParams CreateParams {
            get { var p=base.CreateParams;p.ExStyle|=0x08000080;return p; }
        }
        public Color Expected(int globalX) {
            if(Pattern<2) {
                bool red=globalX<SplitScreenX;if(Pattern==1)red=!red;
                return red?Color.FromArgb(220,48,64):Color.FromArgb(32,112,224);
            }
            int x=globalX-Left;
            return ((x%4)<2)==(Pattern==2)?Color.Black:Color.White;
        }
        protected override void OnPaint(PaintEventArgs e) {
            if(Pattern<2) {
                e.Graphics.Clear(Expected(Left));
                int split=Math.Max(0,Math.Min(ClientSize.Width,SplitScreenX-Left));
                using(var b=new SolidBrush(Expected(Left+split)))e.Graphics.FillRectangle(b,split,0,ClientSize.Width-split,ClientSize.Height);
            } else {
                e.Graphics.Clear(Color.White);
                for(int x=Pattern==2?0:2;x<ClientSize.Width;x+=4)e.Graphics.FillRectangle(Brushes.Black,x,0,2,ClientSize.Height);
            }
        }
    }
    static void Check(bool ok,string m){if(!ok)throw new InvalidOperationException(m);}
    static uint Owner(IntPtr h){uint p;GetWindowThreadProcessId(h,out p);return p;}
    static IntPtr Root(int x,int y){return GetAncestor(WindowFromPoint(new P{X=x,Y=y}),2);}
    static IntPtr Find(uint pid){
        IntPtr found=IntPtr.Zero;int count=0;
        EnumProc cb=delegate(IntPtr h,IntPtr unused){
            if(Owner(h)==pid&&IsWindowVisible(h)){
                var s=new StringBuilder(256);GetWindowText(h,s,s.Capacity);
                if(s.ToString()=="Doubao Glass Preview - no microphone"){found=h;count++;}
            }return true;
        };
        Check(EnumWindows(cb,IntPtr.Zero)&&count<=1,"Preview enumeration failed.");return found;
    }
    static void Pump(int ms){var t=Stopwatch.StartNew();do{Application.DoEvents();System.Threading.Thread.Sleep(10);}while(t.ElapsedMilliseconds<ms);}
    static void Stop(Process p,IntPtr h){
        if(p==null||p.HasExited)return;
        if(h!=IntPtr.Zero&&Owner(h)==(uint)p.Id)PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);
        if(!p.WaitForExit(2500)){p.Kill();Check(p.WaitForExit(5000),"Test preview did not exit.");}
    }
    static Bitmap Shot(Rectangle crop,IntPtr expected,Paper paper,Point center){
        Check(Root(center.X,center.Y)==expected&&Root(crop.Left+3,crop.Top+3)==paper.Handle,"Test area is covered by another window.");
        var image=new Bitmap(crop.Width,crop.Height,PixelFormat.Format32bppRgb);
        try{
            using(var g=Graphics.FromImage(image)){
                IntPtr src=GetDC(IntPtr.Zero),dst=IntPtr.Zero;
                try{Check(src!=IntPtr.Zero,"Screen DC unavailable.");dst=g.GetHdc();
                    Check(BitBlt(dst,0,0,crop.Width,crop.Height,src,crop.Left,crop.Top,0x40CC0020),"Capture failed.");
                }finally{if(dst!=IntPtr.Zero)g.ReleaseHdc(dst);if(src!=IntPtr.Zero)ReleaseDC(IntPtr.Zero,src);}
            }
            Check(Root(center.X,center.Y)==expected,"Capture layering changed.");return image;
        }catch{image.Dispose();throw;}
    }
    static void ValidateInput(Bitmap image,Rectangle crop,Paper paper){
        // A complete external row checks every stripe, not just an average that
        // would make an inverted black/white pattern look unchanged.
        for(int x=4;x<image.Width-4;x++){
            var a=image.GetPixel(x,4);var b=paper.Expected(crop.Left+x);
            Check(Math.Abs(a.R-b.R)<=2&&Math.Abs(a.G-b.G)<=2&&Math.Abs(a.B-b.B)<=2,"Actual background differs from requested pattern.");
        }
    }
    static double[] Rgb(Bitmap b,Point p){var c=b.GetPixel(p.X,p.Y);return new double[]{c.R,c.G,c.B};}
    static double[] Mean(Bitmap b,List<Point> points){
        double[] sum=new double[3];foreach(var p in points){var c=Rgb(b,p);for(int k=0;k<3;k++)sum[k]+=c[k];}
        for(int k=0;k<3;k++)sum[k]=Math.Round(sum[k]/points.Count,3);return sum;
    }
    static double Delta(Bitmap a,Bitmap b,List<Point> points){
        double sum=0;foreach(var p in points){var x=Rgb(a,p);var y=Rgb(b,p);for(int k=0;k<3;k++)sum+=Math.Abs(x[k]-y[k]);}
        return Math.Round(sum/(points.Count*3),4);
    }
    static double Response(Bitmap a1,Bitmap a2,Bitmap b1,Bitmap b2,List<Point> points){
        double sum=0;foreach(var p in points){var a=Rgb(a1,p);var aa=Rgb(a2,p);var b=Rgb(b1,p);var bb=Rgb(b2,p);
            for(int k=0;k<3;k++)sum+=Math.Abs((a[k]+aa[k]-b[k]-bb[k])/2.0);}
        return Math.Round(sum/(points.Count*3),4);
    }
    static double FineProjection(Bitmap a,Bitmap ar,Bitmap b,Bitmap ia,Bitmap ib,List<Point> points){
        // Signed least-squares slope of output A-B onto actual input A-B,
        // with a fitted constant offset per channel. Uniform color drift is not
        // misreported as transmitted stripe detail. Not an optical blur radius.
        double num=0,den=0;
        for(int k=0;k<3;k++){
            double sx=0,sy=0,sxx=0,sxy=0;
            foreach(var p in points){double x=Rgb(ia,p)[k]-Rgb(ib,p)[k];double y=(Rgb(a,p)[k]+Rgb(ar,p)[k])/2-Rgb(b,p)[k];
                sx+=x;sy+=y;sxx+=x*x;sxy+=x*y;}
            num+=sxy-sx*sy/points.Count;den+=sxx-sx*sx/points.Count;
        }Check(den>1,"Fine input has no useful variance.");return Math.Round(num/den,5);
    }
    static Case RunCase(string exe,string directory,string backend,string theme,bool paperFirst){
        var result=new Case{Name=backend+"-"+theme+"-"+(paperFirst?"paper-first":"preview-first"),Order=paperFirst?"paper-first":"preview-first"};
        Process preview=null;Paper paper=null;IntPtr h=IntPtr.Zero,region=IntPtr.Zero;
        var shots=new Dictionary<string,Bitmap>();var observations=new List<Sample>();
        var inputs=new List<Bitmap>();System.Threading.Tasks.Task<string> logs=null,output=null;
        var age=Stopwatch.StartNew();
        try{
            var screen=Screen.PrimaryScreen.Bounds;
            int pw=Math.Min(800,screen.Width),ph=Math.Min(340,screen.Height);
            paper=new Paper();paper.SplitScreenX=screen.Left+screen.Width/2;
            paper.Bounds=new Rectangle(screen.Left+(screen.Width-pw)/2,screen.Bottom-ph,pw,ph);
            if(paperFirst){paper.Show();paper.Refresh();Pump(600);}
            var si=new ProcessStartInfo(exe,"--glass-backend="+backend+" --theme="+theme+" --content=optimizing --seconds=120"){
                UseShellExecute=false,CreateNoWindow=true,WorkingDirectory=Path.GetDirectoryName(exe),RedirectStandardError=true,RedirectStandardOutput=true};
            si.EnvironmentVariables.Remove("GPUI_DISABLE_DIRECT_COMPOSITION");
            age.Restart();preview=Process.Start(si);Check(preview!=null,"Cannot start test preview.");
            logs=preview.StandardError.ReadToEndAsync();output=preview.StandardOutput.ReadToEndAsync();
            var wait=Stopwatch.StartNew();
            while(wait.ElapsedMilliseconds<12000&&!preview.HasExited){h=Find((uint)preview.Id);if(h!=IntPtr.Zero)break;Pump(20);}
            Check(h!=IntPtr.Zero&&!preview.HasExited,"Preview did not appear.");
            // Preserve the old test ordering for the comparison arm, but do not
            // hide early display samples in the paper-first arm behind a warmup.
            if(!paperFirst)Pump(1500);
            R r;Check(GetWindowRect(h,out r),"Cannot read window geometry.");
            var outer=Rectangle.FromLTRB(r.L,r.T,r.Right,r.Bottom);
            Check(outer.Width>=30&&outer.Width<=600&&outer.Height>=10&&outer.Height<=250,"Unexpected window size.");
            var crop=Rectangle.Inflate(outer,24,24);Check(screen.Contains(crop)&&paper.Bounds.Contains(crop),"Preview lies outside the synthetic test paper.");
            region=CreateRectRgn(0,0,0,0);Check(region!=IntPtr.Zero,"Cannot allocate region.");
            // The preview installs a provisional region before its first paint.
            // Wait for the renderer's device-pixel capsule, not just any region.
            uint dpi=GetDpiForWindow(h);
            int ew=(int)Math.Round(108.0*dpi/96.0,MidpointRounding.AwayFromZero);
            int eh=(int)Math.Round(26.0*dpi/96.0,MidpointRounding.AwayFromZero);
            wait.Restart();bool ready=false;R box=new R();
            while(wait.ElapsedMilliseconds<3000&&!preview.HasExited){
                ready=GetWindowRgn(h,region)>1&&GetRgnBox(region,out box)>1
                    &&Math.Abs(box.Right-box.L-(ew-1))<=1&&Math.Abs(box.Bottom-box.T-(eh-1))<=1;
                if(ready)break;Pump(20);
            }
            Check(ready,"The device-pixel capsule region did not become ready.");
            double cx=(box.L+box.Right)/2.0,cy=(box.T+box.Bottom)/2.0,w=box.Right-box.L,ht=box.Bottom-box.T;
            var center=new Point(outer.Left+(int)cx,outer.Top+(int)cy);
            Check(Math.Abs(center.X-paper.SplitScreenX)<=2,"Unexpected split/preview alignment; no paper repositioned.");
            var left=new List<Point>();var right=new List<Point>();var points=new List<Point>();
            for(int y=box.T;y<box.Bottom;y++)for(int x=box.L;x<box.Right;x++){
                double dx=Math.Abs(x+0.5-cx),dy=Math.Abs(y+0.5-cy);
                if(dx>w*.34&&dx<w*.42&&dy<ht*.16&&PtInRegion(region,x,y)){
                    var p=new Point(x+outer.Left-crop.Left,y+outer.Top-crop.Top);points.Add(p);if(x+0.5<cx)left.Add(p);else right.Add(p);
                }
            }Check(left.Count>=20&&right.Count>=20,"Insufficient safe sample pixels.");result.ProbePixels=points.Count;
            if(!paperFirst)paper.Show();
            Check(SetWindowPos(h,new IntPtr(-1),0,0,0,0,0x0213),"Cannot stack preview.");
            Check(SetWindowPos(paper.Handle,h,paper.Left,paper.Top,paper.Width,paper.Height,0x0250),"Cannot stack paper.");
            string[] names={"first-a","same-a-repaint","b1","a1","b2","a2","fine-a","fine-b","fine-a-return"};
            int[] patterns={0,0,1,0,1,0,2,3,2};
            for(int i=0;i<names.Length;i++){
                // No extra refresh during first-a: distinguish waiting from a
                // same-content explicit repaint in the following phase.
                var phase=Stopwatch.StartNew();
                if(i>0){paper.Pattern=patterns[i];paper.Refresh();}
                int[] times=i==0?new int[]{50,250,800,1600,3200}:new int[]{800,1600};
                for(int j=0;j<times.Length;j++){
                    while(phase.ElapsedMilliseconds<times[j])Pump(10);
                    R now;Check(!preview.HasExited&&GetWindowRect(h,out now)&&now.L==r.L&&now.T==r.T&&now.Right==r.Right&&now.Bottom==r.Bottom,"Preview moved or exited.");
                    R currentBox;Check(GetWindowRgn(h,region)>1&&GetRgnBox(region,out currentBox)>1&&currentBox.L==box.L&&currentBox.T==box.T&&currentBox.Right==box.Right&&currentBox.Bottom==box.Bottom,"Capsule region changed during measurement.");
                    string key=names[i]+"-"+times[j];string file=result.Name+"-"+key+".png";
                    double started=phase.Elapsed.TotalMilliseconds;
                    var shot=Shot(crop,h,paper,center);shots.Add(key,shot);
                    double copied=phase.Elapsed.TotalMilliseconds;
                    ValidateInput(shot,crop,paper);
                    observations.Add(new Sample{Phase=names[i],File=file,RequestedMs=times[j],CopyStartedMs=Math.Round(started,3),CopyDoneMs=Math.Round(copied,3),ProcessAgeMs=Math.Round(age.Elapsed.TotalMilliseconds,3),InputValidated=true,LeftRgb=Mean(shot,left),RightRgb=Mean(shot,right)});
                    shot.Save(Path.Combine(directory,file),ImageFormat.Png);
                }
            }
            Stop(preview,h);Pump(300);
            for(int i=0;i<4;i++){
                paper.Pattern=i;paper.Refresh();Pump(400);
                var input=Shot(crop,paper.Handle,paper,center);inputs.Add(input);ValidateInput(input,crop,paper);
                input.Save(Path.Combine(directory,result.Name+"-input-"+i+".png"),ImageFormat.Png);
            }
            double coarseInput=Delta(inputs[0],inputs[1],points),fineInput=Delta(inputs[2],inputs[3],points);
            Check(coarseInput>80&&fineInput>200,"Background input contrast is invalid.");
            result.FirstA800To3200=Delta(shots["first-a-800"],shots["first-a-3200"],points);
            result.SameARepaintChange=Delta(shots["first-a-3200"],shots["same-a-repaint-1600"],points);
            result.FirstAToWarmA=Delta(shots["first-a-3200"],shots["a2-1600"],points);
            result.WarmResponse=Response(shots["a1-1600"],shots["a2-1600"],shots["b1-1600"],shots["b2-1600"],points);
            result.WarmGain=Math.Round(result.WarmResponse.Value/coarseInput,4);
            result.WarmReturnA=Delta(shots["a1-1600"],shots["a2-1600"],points);
            result.WarmReturnB=Delta(shots["b1-1600"],shots["b2-1600"],points);
            result.FineGain=Math.Round(Response(shots["fine-a-1600"],shots["fine-a-return-1600"],shots["fine-b-1600"],shots["fine-b-1600"],points)/fineInput,4);
            result.FineSignedGain=FineProjection(shots["fine-a-1600"],shots["fine-a-return-1600"],shots["fine-b-1600"],inputs[2],inputs[3],points);
            result.FineReturn=Delta(shots["fine-a-1600"],shots["fine-a-return-1600"],points);
            result.AllInputsValidated=true;
        }catch(Exception e){result.Error=e.Message;}
        finally{
            try{Stop(preview,h);}catch(Exception e){result.Error=(result.Error??"")+" Cleanup: "+e.Message;}
            if(paper!=null){paper.Close();paper.Dispose();Application.DoEvents();}
            foreach(var image in shots.Values)image.Dispose();foreach(var image in inputs)image.Dispose();if(region!=IntPtr.Zero)DeleteObject(region);
            result.Stopped=preview==null||preview.HasExited;result.Samples=observations.ToArray();
            try{
                if(result.Stopped&&logs!=null&&logs.Wait(2000)){
                    string text=logs.Result??"";File.WriteAllText(Path.Combine(directory,result.Name+".log"),text,Encoding.UTF8);
                    var m=Regex.Matches(text,@"\[glass\] requested=\w+ selected=(\w+)");if(m.Count>0)result.Selected=m[m.Count-1].Groups[1].Value;
                    result.HostReported=text.Contains("[glass-host] native brush attached");result.Log=text.Length>700?text.Substring(text.Length-700):text;
                }
                if(result.Stopped&&output!=null)output.Wait(2000);
            }catch(Exception e){result.Error=(result.Error??"")+" Log: "+e.Message;}
            if(preview!=null)preview.Dispose();
        }return result;
    }
    public static Report Run(string exe,string directory){
        Check(IntPtr.Size==8&&File.Exists(exe),"64-bit Windows PowerShell and GlassPreview.exe are required.");
        var existing=Process.GetProcessesByName("GlassPreview");int n=existing.Length;foreach(var p in existing)p.Dispose();
        Check(n==0,"An existing preview is running; none was changed.");Check(!Directory.Exists(directory),"Refusing to overwrite output directory.");
        var report=new Report{Directory=directory};var cases=new List<Case>();IntPtr old=IntPtr.Zero,focus=GetForegroundWindow();
        try{
            Directory.CreateDirectory(directory);old=SetThreadDpiAwarenessContext(new IntPtr(-4));Check(old!=IntPtr.Zero,"Cannot enter physical-pixel context.");
            foreach(string theme in new string[]{"light","dark"}){
                foreach(bool first in new bool[]{false,true}){var c=RunCase(exe,directory,"native",theme,first);cases.Add(c);Check(c.Error==null&&c.Stopped,c.Error??"Case cleanup failed.");}
                var solid=RunCase(exe,directory,"solid",theme,true);cases.Add(solid);Check(solid.Error==null&&solid.Stopped,solid.Error??"Solid cleanup failed.");
            }report.Completed=true;
        }catch(Exception e){report.Error=e.Message;}
        finally{if(old!=IntPtr.Zero)SetThreadDpiAwarenessContext(old);report.FocusPreserved=focus==GetForegroundWindow();report.Cases=cases.ToArray();}
        return report;
    }
}
'@
}
if ($CompileOnly) { Write-Output 'Diagnostic C# compiled; no window, microphone or capture started.'; $global:LASTEXITCODE=0; return }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory=Join-Path $env:TEMP ('doubao-startup-audit-'+[Guid]::NewGuid().ToString('N'))
}
$exe=Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
$result=[GlassStartupAudit16]::Run($exe,$OutputDirectory)
$result.ExecutableSha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
if (Test-Path -LiteralPath $OutputDirectory -PathType Container) {
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'result.json'),($result|ConvertTo-Json -Depth 8 -Compress))
}
# Keep transport output bounded. Full timing, RGB samples and PNGs stay local.
$cases=@($result.Cases | Select-Object Name,Selected,Stopped,AllInputsValidated,Error,FirstA800To3200,SameARepaintChange,FirstAToWarmA,WarmResponse,WarmReturnA,WarmReturnB,WarmGain,FineSignedGain,FineReturn)
[ordered]@{Completed=$result.Completed;FocusPreserved=$result.FocusPreserved;Error=$result.Error;Directory=$result.Directory;ExecutableSha256=$result.ExecutableSha256;Interpretation=$result.Interpretation;Cases=$cases}|ConvertTo-Json -Depth 5 -Compress
if (-not $result.Completed) { throw ('Startup audit failed: '+$result.Error) }
$global:LASTEXITCODE=0
