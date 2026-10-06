const ToDoCATS = [
  {id:true, color: "#e05555", label:"Задача"},
  {id:false, color: "#4caf82", label:"Заметка"}
];

const CATS=[
  {id:'sales',label:'Скидки',      icon:'🔥',color:'#f59e0b'},
  {id:'bp',    label:'Battle Pass',icon:'⚔', color:'#8b5cf6'},
  {id:'patch', label:'Патч',       icon:'🆕',color:'#10b981'},
  {id:'daily', label:'Ивент',      icon:'⚡',color:'#3b82f6'},
  {id:'preorder',label:'Предзаказы',     icon:'⏳',color:'#ec4899'},
  {id:'other', label:'Прочее',     icon:'▪', color:'#6b7280'},
  {id:'notes', label:'Заметки',     icon:'📝', color:'#ff6b35'},
];

const MRU=['Январь','Февраль','Март','Апрель','Май','Июнь','Июль','Август','Сентябрь','Октябрь','Ноябрь','Декабрь'];

const MSH=['ЯНВ','ФЕВ','МАР','АПР','МАЙ','ИЮН','ИЮЛ','АВГ','СЕН','ОКТ','НОЯ','ДЕК'];

const MMIN=['янв','фев','мар','апр','май','июн','июл','авг','сен','окт','ноя','дек'];

const TODAY=new Date().setHours(0,0,0,0);
const diff=(a,b)=>Math.round((b-a)/86400000);

let S={
  events:[],hiddenCats:new Set(),
  viewStart:null,viewDays:90,
  editingId:null,selectedCat:'sales',popupEventId:null,gameName:'',
  selectToDoCat: true,
  nowOff:diff(90,TODAY)
};

const DAY_H=40, LANE_W=90;

const uid=()=>Math.random().toString(36).slice(2)+Date.now().toString(36);
const d2s=d=>d.toISOString().slice(0,10);
const s2d=s=>new Date(s+'T00:00:00');
const addD=(d,n)=>{const r=new Date(d);r.setDate(r.getDate()+n);return r};

function linkPopup(link){
  window.open(link, '_blank', 'noopener,noreferrer');
}

function downloadJSON(data, filename = 'data.json') {
  const json = JSON.stringify(data, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);

  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();

  URL.revokeObjectURL(url);
}

function loadJSON(callback) {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json';
  input.onchange = e => {
    const reader = new FileReader();
    reader.onload = ev => callback(JSON.parse(ev.target.result));
    reader.readAsText(e.target.files[0]);
  };
  input.click();
}

function exportData(){
  let s_for_ex = {
    events:S.events,hiddenCats:new Set(),
    viewStart:null,viewDays:90,
    editingId:null, selectedToDoCat: true,selectedCat:'sales',popupEventId:null,gameName:S.gameName,
  };
  downloadJSON(s_for_ex, "settings.json");  
}

function importData() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json';
  
  input.onchange = e => {
    const file = e.target.files[0];
    if (!file) return;
    
    const reader = new FileReader();
    reader.onload = ev => {
      try {
        const imported = JSON.parse(ev.target.result);
        S.events = imported.events;
        S.gameName = imported.gameName;
        save();render();
        document.getElementById('gameInput').value=S.gameName;
        // обновляешь свой объект
        //Object.assign(myData, imported);
        // или если нужно заменить целиком:
        // myData = imported;
      } catch {
        alert('Неверный формат файла');
      }
    };
    reader.readAsText(file);
  };
  
  input.click();
}

function load(){
  try{
    const d=JSON.parse(localStorage.getItem('storage_schedule')||'{}');
    d.events.forEach(ev => {
      ev.link = ev.link == undefined ? "" :ev.link;
    })
    if(d.events) S.events=d.events;
    if(d.hiddenCats) S.hiddenCats=new Set(d.hiddenCats);
    if(d.gameName){S.gameName=d.gameName;document.getElementById('gameInput').value=d.gameName;}
  }catch(e){}
}

function save(){
  localStorage.setItem('storage_schedule',JSON.stringify({events:S.events,hiddenCats:[...S.hiddenCats],gameName:S.gameName}));
}

document.getElementById('gameInput').addEventListener('input',e=>{S.gameName=e.target.value;save();});

function initView(){
  const now=new Date();now.setHours(0,0,0,0);
  const st=addD(now,-60);st.setDate(1);
  S.viewStart=st;S.viewDays=150;
}

function buildCats(){
  return CATS.filter(c=>!S.hiddenCats.has(c.id)).map(cat=>{
    const evs=S.events.filter(e=>e.cat===cat.id).sort((a,b)=>a.start.localeCompare(b.start));
    const lanes=[];
    evs.forEach(ev=>{
      let placed=false;
      for(let l=0;l<lanes.length;l++){
        const last=lanes[l][lanes[l].length-1];
        if(last.end<ev.start){lanes[l].push(ev);placed=true;break;}
      }
      if(!placed) lanes.push([ev]);
    });
    return {...cat,evs,lanes};
  });
}

function render(){
  renderLegend();
  const cats=buildCats();
  renderColHeader(cats);
  renderTimeline(cats);
  renderStatus();
}

function renderLegend(){
  document.getElementById('legend').innerHTML=CATS.map(c=>`
    <div class="leg-item ${S.hiddenCats.has(c.id)?'off':''}" onclick="toggleCat('${c.id}')">
      <div class="leg-dot" style="background:${c.color}"></div>${c.icon} ${c.label}
    </div>`).join('');
}

function toggleCat(id){S.hiddenCats.has(id)?S.hiddenCats.delete(id):S.hiddenCats.add(id);save();render();}

function renderColHeader(cats){
  // update nav label
  const vs=S.viewStart, ve=addD(vs,S.viewDays-1);
  document.getElementById('navLabel').textContent=
    vs.getMonth()===ve.getMonth()
      ?MRU[vs.getMonth()]+' '+vs.getFullYear()
      :MSH[vs.getMonth()]+' – '+MSH[ve.getMonth()]+' '+ve.getFullYear();

  document.getElementById('chCats').innerHTML=cats.map(cat=>{
    const lc=Math.max(1,cat.lanes.length);
    const w=lc*LANE_W;
    return `<div class="cat-head" style="width:${w}px;min-width:${w}px;border-left:2px solid ${cat.color}25">
      <div class="ch-dot" style="background:${cat.color}"></div>
      <span style="color:${cat.color}">${cat.icon}</span>
      <span>${cat.label}</span>
      ${lc>1?`<span style="color:var(--text3);font-size:9px">×${lc}</span>`:''}
    </div>`;
  }).join('');
}

function renderTimeline(cats){
  const vs=S.viewStart, vd=S.viewDays;
  const today=new Date();today.setHours(0,0,0,0);
  const todayS=d2s(today);

  // axis
  const ax=document.getElementById('timeAxis');
  ax.innerHTML='';ax.style.height=(vd*DAY_H)+'px';
  for(let i=0;i<vd;i++){
    const d=addD(vs,i), ds=d2s(d);
    const dow=d.getDay(), isWE=dow===0||dow===6;
    const isTd=ds===todayS, isMS=d.getDate()===1;
    const cell=document.createElement('div');
    cell.className='ax-day'+(isTd?' today':'')+(isWE?' weekend':'')+(isMS?' ms':'');
    cell.innerHTML=(isMS||i===0?`<span class="ax-month">${MSH[d.getMonth()]}</span>`:'')+`<span class="ax-num">${d.getDate()}</span>`;
    ax.appendChild(cell);
  }

  // total grid width
  let totalW=0;
  const catX=[];
  cats.forEach(c=>{catX.push(totalW);totalW+=Math.max(1,c.lanes.length)*LANE_W;});
  const totalH=vd*DAY_H;

  const gi=document.getElementById('gridInner');
  gi.innerHTML='';gi.style.width=totalW+'px';gi.style.height=totalH+'px';

  // h-lines
  for(let i=0;i<vd;i++){
    const d=addD(vs,i), ds=d2s(d), isMS=d.getDate()===1, isTd=ds===todayS;
    const ln=document.createElement('div');
    ln.className='h-line'+(isMS?' ms':'')+(isTd?' tl':'');
    ln.style.top=(i*DAY_H)+'px';
    gi.appendChild(ln);
    if(isTd){
      const lbl=document.createElement('div');
      lbl.className='today-lbl';lbl.style.top=(i*DAY_H+2)+'px';lbl.textContent='TODAY';
      gi.appendChild(lbl);
    }
  }

  // col dividers
  cats.forEach((c,ci)=>{
    const div=document.createElement('div');
    div.className='col-div';div.style.left=catX[ci]+'px';
    div.style.borderLeft=`2px solid ${c.color}20`;
    gi.appendChild(div);
    // lane sub-dividers
    for(let l=1;l<c.lanes.length;l++){
      const sd=document.createElement('div');
      sd.className='col-div';sd.style.left=(catX[ci]+l*LANE_W)+'px';
      sd.style.background='rgba(255,255,255,0.04)';
      gi.appendChild(sd);
    }
  });

  // click to add
  gi.addEventListener('click',e=>{
    if(e.target!==gi&&!e.target.classList.contains('h-line')&&!e.target.classList.contains('col-div')&&!e.target.classList.contains('today-lbl')) return;
    const rect=gi.getBoundingClientRect();
    const bs=document.getElementById('bodyScroll');
    const cy=e.clientY-rect.top+bs.scrollTop;
    const cx=e.clientX-rect.left+bs.scrollLeft;
    const dayIdx=Math.max(0,Math.min(vd-1,Math.floor(cy/DAY_H)));
    const clickDate=addD(vs,dayIdx);
    let clickedCat='sales';
    for(let ci=cats.length-1;ci>=0;ci--){if(cx>=catX[ci]){clickedCat=cats[ci].id;break;}}
    openModal(null,d2s(clickDate),clickedCat);
  });

  // event bars
  cats.forEach((cat,ci)=>{
    cat.lanes.forEach((lane,li)=>{
      lane.forEach(ev=>{
        const evS=s2d(ev.start), evE=s2d(ev.end);
        const startOff=diff(vs,evS), dur=diff(evS,evE)+1;
        const y=startOff*DAY_H, h=dur*DAY_H;
        if(y+h<0||y>totalH) return;
        const x=catX[ci]+li*LANE_W;
        const PAD=3;
        const bar=document.createElement('div');
        bar.className='ev-bar';
        bar.style.cssText=`left:${x+PAD}px;width:${LANE_W-PAD*2}px;top:${Math.max(0,y)}px;height:${(y<0?h+y:h)-2}px;background:${cat.color}1e;border-top-color:${cat.color};color:${cat.color}`;
        bar.innerHTML=`<span class="ev-icon">${cat.icon}${!ev.is_done || ev.is_done == undefined ? '🔴' : ''}</span><span class="ev-title">${ev.title}</span>`;
        bar.dataset.id=ev.id;
        bar.addEventListener('click',e=>{e.stopPropagation();showPopup(ev.id,e);});
        gi.appendChild(bar);
      });
    });
  });

  // scroll to today
  //const off=diff(vs,today);
  //const bs=document.getElementById('bodyScroll');
  //if(off>=0&&off<vd) setTimeout(()=>{bs.scrollTop=Math.max(0,off*DAY_H-120);},0);
}

function renderStatus(){
  const today=new Date();today.setHours(0,0,0,0);
  const week=addD(today,7);
  const vis=S.events.filter(e=>!S.hiddenCats.has(e.cat));
  const active=vis.filter(e=>s2d(e.start)<=today&&s2d(e.end)>=today);
  const soon=vis.filter(e=>s2d(e.start)>today&&s2d(e.start)<=week);
  document.getElementById('stTotal').textContent=S.events.length;
  document.getElementById('stActive').textContent=active.length;
  document.getElementById('stSoon').textContent=soon.length;
}

// sync header with horizontal scroll
document.getElementById('bodyScroll').addEventListener('scroll',function(){
  document.getElementById('chCats').scrollLeft=this.scrollLeft;
});

// POPUP
let pTimer;
function showPopup(id,e){
  const ev=S.events.find(x=>x.id===id);if(!ev) return;
  const cat=CATS.find(c=>c.id===ev.cat);
  S.popupEventId=id;
  document.getElementById('pcCat').textContent=cat.icon+' '+cat.label.toUpperCase();
  document.getElementById('pcCat').style.color=cat.color;
  document.getElementById('pcTitle').textContent=ev.title;
  document.getElementById('pcDates').textContent=fmt(ev.start)+' → '+fmt(ev.end);
  let lw = document.getElementById('pc-link');
  if (ev.link) {
    lw.classList.remove('hidden');
    lw.onclick = () => linkPopup(ev.link);
  } else {
    lw.classList.add('hidden');
  }
  document.getElementById('pcDur').textContent=(diff(s2d(ev.start),s2d(ev.end))+1)+' дн.';
  const pn=document.getElementById('pcNote');
  if(ev.note){pn.textContent=ev.note;pn.classList.remove('hidden');}else pn.classList.add('hidden');
  const pop=document.getElementById('popup');
  pop.style.left=Math.min(e.clientX+12,window.innerWidth-295)+'px';
  pop.style.top=Math.min(e.clientY-10,window.innerHeight-250)+'px';
  
  pop.classList.add('show');clearTimeout(pTimer);
}
function hidePopup(){pTimer=setTimeout(()=>document.getElementById('popup').classList.remove('show'),200);}
document.addEventListener('click',e=>{if(!e.target.closest('.popup')&&!e.target.closest('.ev-bar')) hidePopup();});
document.getElementById('popup').addEventListener('mouseenter',()=>clearTimeout(pTimer));
document.getElementById('popup').addEventListener('mouseleave',hidePopup);
function editFromPopup(){hidePopup();openModal(S.popupEventId);}
function deleteFromPopup(){if(!confirm('Удалить ивент?')) return;S.events=S.events.filter(e=>e.id!==S.popupEventId);save();hidePopup();render();}
function fmt(s){const d=s2d(s);return d.getDate()+' '+MMIN[d.getMonth()];}

function saveData(){
  save();
}
// MODAL
function openModal(id,dateStr,catId){
  S.editingId=id||null;
  document.getElementById('overlay').classList.remove('hidden');
  if(id){
    const ev=S.events.find(e=>e.id===id);
    document.getElementById('modalTitle').textContent='РЕДАКТИРОВАТЬ';
    document.getElementById('fTitle').value=ev.title;
    document.getElementById('fStart').value=ev.start;
    document.getElementById('fEnd').value=ev.end;
    document.getElementById('fNote').value=ev.note||'';
    document.getElementById('fLink').value=ev.link||'';
    S.selectedCat=ev.cat;
    S.selectedToDoCat=!ev.is_done;
    document.getElementById('btnDel').classList.remove('hidden');
  }else{
    document.getElementById('modalTitle').textContent='НОВЫЙ ИВЕНТ';
    document.getElementById('fTitle').value='';
    const td=dateStr||d2s(new Date());
    document.getElementById('fStart').value=td;
    document.getElementById('fEnd').value=td;
    document.getElementById('fNote').value='';
    document.getElementById('fLink').value='';
    S.selectedCat=catId||'sales';
    S.selectedToDoCat=true;
    document.getElementById('btnDel').classList.add('hidden');
  }
  renderCatPicker();
  renderToDo();
  setTimeout(()=>document.getElementById('fTitle').focus(),50);
}
function renderCatPicker(){
  document.getElementById('catPicker').innerHTML=CATS.map(c=>`
    <div class="cat-opt" style="${c.id===S.selectedCat?'background:'+c.color+'33;border-color:'+c.color+';color:'+c.color:''}"
      onclick="selCat('${c.id}')">
      <div class="cat-swatch" style="background:${c.color}"></div>${c.label}
    </div>`).join('');
}
function renderToDo(){
  document.getElementById('needToDo').innerHTML=ToDoCATS.map(c=>`
    <div class="cat-opt" style="${c.id===S.selectedToDoCat?'background:'+c.color+'33;border-color:'+c.color+';color:'+c.color:''}"
      onclick="selToDoCat(${c.id})">
      <div class="cat-swatch" style="background:${c.color}"></div>${c.label}
    </div>`).join('');
}
function selCat(id){S.selectedCat=id;renderCatPicker();}
function selToDoCat(id){console.log(id,S.selectedToDoCat);S.selectedToDoCat=id;renderToDo();}
function closeModal(){document.getElementById('overlay').classList.add('hidden');}
function saveEvent(){
  const title=document.getElementById('fTitle').value.trim();
  if(!title){document.getElementById('fTitle').focus();return;}
  const start=document.getElementById('fStart').value, end=document.getElementById('fEnd').value;
  if(!start||!end) return;
  if(end<start){alert('Дата конца раньше начала');return;}
  const ev={id:S.editingId||uid(),title,cat:S.selectedCat,start,end,note:document.getElementById('fNote').value.trim(),is_done:S.selectedToDoCat ? false : true,link: document.getElementById('fLink').value.trim()};
  if(S.editingId){const i=S.events.findIndex(e=>e.id===S.editingId);S.events[i]=ev;}
  else S.events.push(ev);
  save();closeModal();render();
}
function deleteEvent(){
  if(!S.editingId||!confirm('Удалить?')) return;
  S.events=S.events.filter(e=>e.id!==S.editingId);save();closeModal();render();
}
document.getElementById('overlay').addEventListener('click',e=>{if(e.target===document.getElementById('overlay')) closeModal();});
document.addEventListener('keydown',e=>{if(e.key==='Escape') closeModal();if(e.key==='n'&&!e.target.closest('input,textarea')) openModal();});

// NAV
function shiftView(dir){S.viewStart=addD(S.viewStart,dir*30);render();}
function goToday(){
  initView();render();
  const today=new Date();today.setHours(0,0,0,0);
  const off=diff(S.viewStart,today);
  document.getElementById('bodyScroll').scrollTop=Math.max(0,off*DAY_H-120);
}

// SAMPLE DATA
function seedSample(){
  if(S.events.length) return;
  const t=new Date();t.setHours(0,0,0,0);
  const s=n=>d2s(addD(t,n)), td=d2s(t);
  S.events=[];
  save();
}

load();
seedSample();
goToday();
console.log('Скрипт подключён');