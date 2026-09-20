"""Render an original 64-second, 1080p motion-graphics promo; no external media.

Requires Pillow, numpy and imageio-ffmpeg. Windows fonts are rendered locally,
not redistributed. Run: python marketing/promo/render.py --out target/promo
Use --preview to render story frames only. Illustrations are interaction demos,
not recordings of successful third-party speech recognition or hardware tests.
"""
from pathlib import Path
import argparse
import math
import subprocess
import wave
from functools import lru_cache

import numpy as np
from PIL import Image, ImageDraw, ImageFont
import imageio_ffmpeg

W, H, FPS, DURATION = 1920, 1080, 24, 64
BG = (16, 20, 19)
WHITE = (242, 244, 234)
MUTED = (158, 173, 161)
LIME = (196, 245, 112)
BLUE = (139, 199, 252)
ORANGE = (247, 181, 109)
STOPS = [0, 5, 11, 17, 24, 32, 40, 48, 54, 64]
FONT_DIR = Path('C:/Windows/Fonts')


@lru_cache(maxsize=100)
def font(size, bold=False, latin=False):
    name = 'bahnschrift.ttf' if latin else ('msyhbd.ttc' if bold else 'msyh.ttc')
    return ImageFont.truetype(str(FONT_DIR / name), size)


def txt(draw, xy, text, size=32, fill=WHITE, bold=False, latin=False, anchor=None):
    draw.text(xy, text, font=font(size, bold, latin), fill=fill, anchor=anchor)


def ease(x):
    x = min(1., max(0., x))
    return 1 - (1-x)**3


def panel(draw, box, fill=(28, 34, 30), outline=(64, 77, 64), radius=24, width=2):
    draw.rounded_rectangle(tuple(map(int, box)), radius=radius, fill=fill, outline=outline, width=width)


def pill(draw, x, y, text, color=LIME, size=25, dark=True):
    f=font(size, True)
    width=int(draw.textlength(text, font=f))+40
    draw.rounded_rectangle((x,y,x+width,y+size+26), radius=(size+26)//2, fill=color if dark else (32,42,32), outline=color)
    draw.text((x+20,y+10),text,font=f,fill=BG if dark else color)
    return width


def chevron(draw, x, y, direction, color, scale=1):
    s=11*scale
    points=[(-s, s/2),(0,-s/2),(s,s/2)]
    a={'up':0,'right':math.pi/2,'down':math.pi,'left':-math.pi/2}[direction]
    pts=[(x+px*math.cos(a)-py*math.sin(a),y+px*math.sin(a)+py*math.cos(a)) for px,py in points]
    draw.line(pts,fill=color,width=max(2,int(4*scale)))


@lru_cache(maxsize=80)
def remote(active='', tilt=0):
    im=Image.new('RGBA',(450,900))
    d=ImageDraw.Draw(im)
    d.rounded_rectangle((67,18,397,875),radius=148,fill=(2,5,4,95))
    d.rounded_rectangle((50,10,360,860),radius=143,fill=(61,68,61),outline=(116,125,107),width=2)
    d.rounded_rectangle((54,13,352,851),radius=139,fill=(36,43,37),outline=(91,100,83),width=2)
    d.arc((59,18,346,845),90,270,fill=(168,177,150),width=2)
    # RC003-inspired illustration; deliberately no Xiaomi logo or exact product claim.
    controls={'power':(120,117,28),'voice':(282,117,28),'home':(118,443,29),'back':(204,443,29),'menu':(290,443,29),'tv':(204,548,31),'plus':(204,658,35),'minus':(204,755,35)}
    d.ellipse((96,187,312,403),fill=(17,24,20),outline=(99,111,86),width=2)
    d.ellipse((154,245,254,345),fill=(47,58,42),outline=(93,108,78),width=2)
    txt(d,(204,293),'OK',25,WHITE,latin=True,anchor='mm')
    for di,x,y in [('up',204,217),('down',204,374),('left',126,296),('right',282,296)]:
        chevron(d,x,y,di,MUTED)
    for key,(x,y,r) in controls.items():
        c=LIME if key==active else (22,28,24)
        d.ellipse((x-r,y-r,x+r,y+r),fill=c,outline=LIME if key==active else (74,88,67),width=2)
        ink=BG if key==active else WHITE
        if key=='power':
            d.arc((x-11,y-10,x+11,y+12),-45,225,fill=ink,width=3);d.line((x,y-14,x,y),fill=ink,width=3)
        elif key=='voice':
            d.rounded_rectangle((x-6,y-14,x+6,y+5),radius=6,outline=ink,width=2);d.arc((x-12,y-9,x+12,y+12),0,180,fill=ink,width=2);d.line((x,y+13,x,y+19),fill=ink,width=2)
        elif key=='home':
            d.line([(x-12,y),(x,y-10),(x+12,y),(x+10,y),(x+10,y+10),(x-10,y+10),(x-10,y),(x-12,y)],fill=ink,width=2)
        elif key=='back':
            d.line([(x+10,y+9),(x+10,y-5),(x-10,y-5),(x-4,y-11),(x-10,y-5),(x-4,y+1)],fill=ink,width=3)
        elif key=='menu':
            for yy in [-7,0,7]: d.line((x-10,y+yy,x+10,y+yy),fill=ink,width=2)
        elif key in ('plus','minus'):
            d.line((x-11,y,x+11,y),fill=ink,width=3)
            if key=='plus':d.line((x,y-11,x,y+11),fill=ink,width=3)
        else: txt(d,(x,y-1),'TV',26,ink,latin=True,anchor='mm')
    txt(d,(204,812),'CONTROL / CREATE',12,MUTED,latin=True,anchor='mm')
    return im.rotate(tilt,resample=Image.Resampling.BICUBIC,expand=True)


def put_remote(im, x, y, height=840, active='', tilt=-10):
    r=remote(active,tilt)
    r=r.resize((int(r.width*height/r.height),int(height)),Image.Resampling.LANCZOS)
    im.paste(r,(int(x),int(y)),r)


def header(d, n, label):
    txt(d,(100,65),'SAYALL / WINDOWS',23,LIME,latin=True)
    txt(d,(1820,65),f'{n:02d}  /  {label}',22,MUTED,latin=True,anchor='ra')
    d.line((100,112,1820,112),fill=(62,73,61),width=1)


def foot(d, line='交互动效示意 · Windows 预览版 · 快捷键作用于当前前台窗口'):
    txt(d,(100,1004),'交互动效示意 · '+line,22,MUTED)


def title(d, text1, text2, y=226, size=91, second=LIME, x=100):
    txt(d,(x,y),text1,size,WHITE,True)
    txt(d,(x,y+size+26),text2,size,second,True)


yy,xx=np.mgrid[0:H,0:W]
glow=np.clip(1-(((xx-1490)/1250)**2+((yy-330)/1100)**2),0,1)
base=np.zeros((H,W,3),dtype=np.uint8)
for c in range(3): base[:,:,c]=BG[c]+(glow*(7 if c!=1 else 12)).astype(np.uint8)
BACKGROUND=Image.fromarray(base)


def frame(t):
    scene=max(i for i,s in enumerate(STOPS[:-1]) if t>=s)
    local=t-STOPS[scene]
    duration=STOPS[scene+1]-STOPS[scene]
    im=BACKGROUND.copy(); d=ImageDraw.Draw(im)
    # A slow technical grid, quiet enough to keep the copy dominant.
    for x in range(1060,1860,100): d.line((x,150,x,960),fill=(32,42,33))
    for y in range(180,960,100): d.line((1030,y,1840,y),fill=(32,42,33))
    header(d,scene+1,['THE QUESTION','A NEW CONTROLLER','WINDOWS EDITION','PUSH TO TALK','SWITCH CONTEXT','EDIT NATURALLY','YOUR DEFAULTS','THREE KEYS','OPEN SOURCE'][scene])
    dy=int((1-ease(local/0.65))*55)
    if scene==0:
        txt(d,(102,200+dy),'VIBE CODING',44,LIME,latin=True)
        txt(d,(96,292+dy),'指挥 AI，',126,WHITE,True)
        txt(d,(96,453+dy),'还得抱着键盘？',112,WHITE,True)
        txt(d,(106,674),'换个姿势。',50,LIME,True)
        # Keys dissolve into a pulse aimed at the next shot.
        for i,label in enumerate(['Ctrl','Shift','Enter']):
            x=1120+i*204; y=730-int(22*math.sin(local*2+i))
            panel(d,(x,y,x+180,y+130),fill=(27,34,29))
            txt(d,(x+90,y+62),label,32,MUTED,latin=True,anchor='mm')
        d.arc((1270,225,1640,595),int(t*42),int(t*42)+270,fill=LIME,width=4)
        txt(d,(1455,414),'?',154,LIME,latin=True,anchor='mm')
        foot(d,'一只蓝牙遥控器，一种与 AI 协作的新姿势。')
    elif scene==1:
        title(d,'把遥控器变成','Agent 控制器。',y=244+dy,size=87)
        txt(d,(104,514),'无线麦 SayAll Windows 版',36,WHITE,True)
        txt(d,(104,582),'开口描述，按键切换，随手修改。',32,MUTED)
        pill(d,104,712,'WINDOWS × VIBE CODING',LIME,size=26)
        put_remote(im,1190+24*math.sin(local*.5),138,795,tilt=-14+int(2*math.sin(local)))
        foot(d,'遥控器外观为原创示意图。')
    elif scene==2:
        # Large Windows motif, drawn directly without external logo assets.
        for a in range(2):
            for b in range(2):d.rectangle((1270+a*194,252+b*194,1446+a*194,428+b*194),fill=LIME)
        title(d,'这次，','在 Windows。',y=220+dy,size=108)
        txt(d,(105,575),'延续 Mac 原版的灵感，',34,MUTED)
        txt(d,(105,628),'基于 SayAll Windows 继续适配与补全。',34,WHITE)
        for i,s in enumerate(['蓝牙语音','按键补全','Codex 预设']):pill(d,105+i*260,753,s,LIME,size=27,dark=False)
        foot(d,'独立开源派生项目；非 SayAll 或 OpenAI 官方发行版。')
    elif scene==3:
        title(d,'按住说。','松开结束。',y=213+dy,size=90)
        txt(d,(104,507),'语音键直达开始与结束。',33,WHITE)
        txt(d,(104,564),'没有双击等待，不打断表达。',30,MUTED)
        panel(d,(1010,235,1796,768))
        pill(d,1050,276,'正在说话' if local<5.4 else '已松开',LIME if local<5.4 else BLUE,size=25)
        for i in range(55):
            h=(16+95*abs(math.sin(i*.62+local*6))*abs(math.sin(i*.2+local*2))) if local<5.4 else 6
            d.rounded_rectangle((1060+i*12,445-h,1065+i*12,445+h),radius=2,fill=LIME)
        txt(d,(1050,632),'“帮我把这个想法做出来。”',35,WHITE,True)
        txt(d,(1050,699),'流程示意 · 非识别结果实录',22,MUTED)
        put_remote(im,730,540,397,'voice' if local<5.4 else '',tilt=14)
        foot(d,'语音需另装 VB-CABLE，并配置目标应用的语音输入与 CABLE Output 麦克风。')
    elif scene==4:
        title(d,'音量键，','变成任务切换键。',y=209+dy,size=77)
        txt(d,(104,472),'音量＋  →  Ctrl + PageUp',34,WHITE)
        txt(d,(104,533),'音量－  →  Ctrl + PageDown',34,WHITE)
        txt(d,(104,636),'在不同聊天之间，来回自如。',31,MUTED)
        idx=min(2,int(max(0,local-1.0)/1.65)%4)
        for i,name in enumerate(['Agent 01  /  构建界面','Agent 02  /  修复逻辑','Agent 03  /  检查改动']):
            y=250+i*168
            panel(d,(1050,y,1810,y+138),fill=(51,65,37) if i==idx else (27,34,29),outline=LIME if i==idx else (64,77,64))
            d.ellipse((1090,y+51,1120,y+81),fill=LIME if i==idx else (74,89,73))
            txt(d,(1150,y+46),name,34,LIME if i==idx else WHITE,True)
        pill(d,1120,825,'切换',LIME,size=28)
        txt(d,(1260,839),'↑   ↓',32,LIME)
        foot(d,'使用 Codex 的上一项／下一项快捷键；也可能经过已打开的标签页。')
    elif scene==5:
        title(d,'该删就删。','该撤就撤。',y=217+dy,size=91)
        txt(d,(104,511),'返回：单按删除，按住连续删。',33,WHITE)
        txt(d,(104,570),'TV 长按：Ctrl + Z 撤销。',33,LIME)
        panel(d,(1000,258,1810,656))
        txt(d,(1040,300),'INPUT',22,MUTED,latin=True)
        text='把多余的文字删掉'
        amount=min(7,max(0,int((local-1.2)*2.1)))
        show=text[:len(text)-amount] if local<5.4 else text
        txt(d,(1040,422),show,49,WHITE,True)
        x=1040+int(d.textlength(show,font=font(49,True)))
        if int(local*3)%2==0:d.rectangle((x+8,425,x+11,485),fill=LIME)
        pill(d,1040,548,'Backspace' if local<5.4 else 'Ctrl + Z',LIME if local<5.4 else BLUE,size=26)
        txt(d,(104,754),'快按返回，不再误触撤销。',32,MUTED)
        foot(d,'日常预设关闭返回双击；撤销范围与次数由当前应用决定。')
    elif scene==6:
        txt(d,(100,191+dy),'预设先上手，',76,WHITE,True)
        txt(d,(100,292+dy),'自定义再顺手。',76,LIME,True)
        txt(d,(104,424),'单击、双击、长按，各有所用。',33,MUTED)
        cols=[104,436,846,1294]
        for x,label in zip(cols,['按键','单击','双击','长按']):txt(d,(x,545),label,27,MUTED,True)
        rows=[('主页','打开 Codex','新建任务','设置'),('菜单','命令菜单','选择模型','待处理任务'),('TV','改动审查','侧边栏','撤销')]
        for i,row in enumerate(rows):
            y=602+i*104
            d.line((104,y-12,1814,y-12),fill=(63,75,60),width=1)
            for j,v in enumerate(row):txt(d,(cols[j],y+5),v,35,LIME if j==0 else WHITE,j==0)
        foot(d,'可修改、禁用或重新绑定；应用预设前自动备份当前按键配置。语音键保持按下／释放。')
    elif scene==7:
        title(d,'补齐 Windows 上的','三枚按键。',y=198+dy,size=80)
        for i,label in enumerate(['返回','音量＋','音量－']):
            x=105+i*260
            panel(d,(x,527,x+228,665),fill=(46,59,37),outline=LIME)
            txt(d,(x+114,594),label,38,LIME,True,anchor='mm')
        panel(d,(104,734,991,864))
        txt(d,(135,770),'显眼开关，按需开启',32,WHITE,True)
        d.rounded_rectangle((813,770,940,824),radius=27,fill=LIME)
        d.ellipse((888,777,935,818),fill=BG)
        put_remote(im,1260,173,732,'back' if local<2 else ('plus' if local<4 else 'minus'),tilt=-11)
        foot(d,'RC003 三键增强为实验性功能；独立 Helper 需要管理员权限。其他型号与完整场景仍待验证。')
    elif scene==8:
        txt(d,(100,166+dy),'全开源。一起把它做得更好。',66,WHITE,True)
        pill(d,104,275,'GPL-3.0-only',LIME,size=25)
        txt(d,(104,377),'WINDOWS / 本项目',24,LIME,latin=False,bold=True)
        txt(d,(100,429),'github.com/MrPhotato/',52,WHITE,latin=True)
        txt(d,(100,493),'remote-mic-app-windows',62,LIME,latin=True)
        d.line((105,592,1810,592),fill=(66,80,62),width=1)
        txt(d,(104,631),'Mac 原版',27,MUTED,True)
        txt(d,(350,626),'github.com/HD838A/remote-mic-app',33,WHITE,latin=True)
        txt(d,(104,696),'Windows 上游',27,MUTED,True)
        txt(d,(350,691),'github.com/GetSayAll/remote-mic-app-windows',33,WHITE,latin=True)
        txt(d,(104,829),'该 Windows 版本由 GPT-6 Astra Ultra 协同制作',34,LIME,True)
        foot(d,'独立开源派生项目 · 持续测试完善中 · 欢迎查看源码、自定义与贡献')
    # Time rail gives the edit a cohesive broadcast identity.
    d=ImageDraw.Draw(im)
    d.rectangle((0,H-6,int(W*t/DURATION),H),fill=LIME)
    # Short dip transitions, no unreadable crossfade overlay.
    fade=min(1.,local/.25,(duration-local)/.2)
    if fade<1: im=Image.blend(Image.new('RGB',(W,H),BG),im,max(0,fade))
    return im


def soundtrack(path):
    sr=48000
    count=sr*DURATION
    data=np.zeros(count,dtype=np.float32)
    rng=np.random.default_rng(64)
    beat=60/108
    # Original restrained electronic pulse: D minor progression, no sampled music.
    for step,at in enumerate(np.arange(0,DURATION,beat)):
        start=int(at*sr); length=min(int(.32*sr),count-start); t=np.arange(length)/sr
        kick=np.sin(2*np.pi*(48*t+8*(1-np.exp(-t*24))))*np.exp(-t*18)*.19
        data[start:start+length]+=kick.astype(np.float32)
        if step%2:
            n=min(int(.12*sr),count-start);tt=np.arange(n)/sr
            data[start:start+n]+=(rng.standard_normal(n)*np.exp(-tt*70)*.035).astype(np.float32)
        notes=[146.832,130.813,116.541,130.813]
        f=notes[(step//8)%4]/2
        n=min(int(beat*.88*sr),count-start);tt=np.arange(n)/sr
        env=np.minimum(tt*35,1)*np.exp(-tt*4)
        data[start:start+n]+=(.13*np.sin(2*np.pi*f*tt)*env).astype(np.float32)
        if step%2==0:
            f=notes[(step//8)%4]*[2,3,4,3][(step//2)%4]
            n=min(int(.65*sr),count-start);tt=np.arange(n)/sr
            data[start:start+n]+=(.035*(np.sin(2*np.pi*f*tt)+.4*np.sin(2*np.pi*f*2*tt))*np.exp(-tt*5)*np.minimum(tt*100,1)).astype(np.float32)
    for at in STOPS[1:-1]:
        start=int(at*sr);n=min(int(.8*sr),count-start);t=np.arange(n)/sr
        data[start:start+n]+=(.048*np.sin(2*np.pi*(620*t+120*t*t))*np.exp(-t*7)*np.minimum(t*60,1)).astype(np.float32)
    t=np.arange(count)/sr
    data*=np.minimum(t/1.5,1)*np.minimum((DURATION-t)/3.5,1)
    pcm=(np.clip(data,-.9,.9)*32767).astype('<i2')
    with wave.open(str(path),'wb') as out:
        out.setnchannels(1);out.setsampwidth(2);out.setframerate(sr);out.writeframes(pcm.tobytes())


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,default=Path('target/promo'));parser.add_argument('--preview',action='store_true');args=parser.parse_args()
    args.out.mkdir(parents=True,exist_ok=True)
    times=[2.5,8,14,20.5,28,36,44,51,58]
    thumbs=[]
    for n,t in enumerate(times):
        shot=frame(t);shot.save(args.out/f'scene-{n+1:02d}.png');thumbs.append(shot.resize((640,360)))
    sheet=Image.new('RGB',(1920,1080))
    for n,thumb in enumerate(thumbs):sheet.paste(thumb,((n%3)*640,(n//3)*360))
    sheet.save(args.out/'storyboard.jpg',quality=92)
    frame(8).save(args.out/'cover.png')
    if args.preview:return
    music=args.out/'original-soundtrack.wav';soundtrack(music)
    output=args.out/'SayAll-Windows-Promo-1080p.mp4'
    command=[imageio_ffmpeg.get_ffmpeg_exe(),'-hide_banner','-loglevel','warning','-y','-f','rawvideo','-vcodec','rawvideo','-pix_fmt','rgb24','-s',f'{W}x{H}','-r',str(FPS),'-i','-','-i',str(music),'-c:v','libx264','-preset','fast','-crf','19','-pix_fmt','yuv420p','-threads','2','-c:a','aac','-b:a','192k','-movflags','+faststart','-shortest',str(output)]
    process=subprocess.Popen(command,stdin=subprocess.PIPE)
    try:
        for i in range(FPS*DURATION):
            process.stdin.write(frame(i/FPS).tobytes())
            if i%(FPS*8)==0:print(f'rendered {i//FPS}/{DURATION}s',flush=True)
    finally:process.stdin.close()
    if process.wait()!=0:raise SystemExit('Encoding failed')
    print(f'completed: {output.resolve()}',flush=True)


if __name__=='__main__':main()
