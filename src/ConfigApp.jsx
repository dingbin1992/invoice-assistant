import { useEffect, useState } from 'preact/hooks';
import { invoke } from './bridge.js';
import { ConfigView } from './ConfigView.jsx';

export function ConfigApp() {
  const [mode, setMode] = useState('view');
  const [categoryList, setCategoryList] = useState([]);
  const [log, setLog] = useState([]);
  const [toast, setToast] = useState(null);

  const addLog = (msg, level = 'info') => {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    setLog(prev => [...prev, { time, msg, level }]);
  };
  const showToast = (msg, type = 'info') => {
    setToast({ msg, type });
    setTimeout(() => setToast(null), 2200);
  };

  useEffect(() => {
    (async () => {
      try {
        const cat = await invoke('read_category');
        setCategoryList(Array.isArray(cat) ? cat : []);
      } catch (e) { addLog(`读取报销类别失败: ${e}`, 'error'); }
    })();
  }, []);

  return (
    <div class="app-main">
      <div class="panel">
        <div class="btn-row">
          <button class={'btn-action' + (mode === 'view' ? ' success' : '')} onClick={() => setMode('view')}>查看报销类别</button>
          <button class={'btn-action' + (mode === 'edit' ? ' success' : '')} onClick={() => setMode('edit')}>修改报销类别</button>
          <div class="flex-spacer" />
          <button class="btn-action warn" onClick={() => history.back()}>返回主页</button>
        </div>
      </div>
      <div class="panel" style="flex:1;display:flex;flex-direction:column;min-height:0;">
        <ConfigView mode={mode} onClose={() => {}} addLog={addLog} showToast={showToast} categoryList={categoryList} />
      </div>
      {toast && <div class={`toast ${toast.type}`}>{toast.msg}</div>}
    </div>
  );
}
