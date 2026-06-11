import { useEffect, useMemo, useState, useCallback } from 'preact/hooks';
import { invoke } from './bridge.js';
import { ConfigView } from './ConfigView.jsx';

const FILTER_ALL = 'all';
const FILTER_OK = 'ok';
const FILTER_NONE = 'none';

export function App() {
  const [workDir, setWorkDir] = useState('');
  const [outputDir, setOutputDir] = useState('');
  const [configDir, setConfigDir] = useState('');
  const [invoices, setInvoices] = useState([]);
  const [filterMode, setFilterMode] = useState(FILTER_ALL);
  const [projectFilter, setProjectFilter] = useState('');
  const [bulkCategory, setBulkCategory] = useState('');
  const [categoryList, setCategoryList] = useState([]);
  const [log, setLog] = useState([]);
  const [toast, setToast] = useState(null);
  const [showConfig, setShowConfig] = useState(false);

  const showToast = (msg, type = 'info') => {
    setToast({ msg, type });
    setTimeout(() => setToast(null), 2200);
  };
  const addLog = (msg, level = 'info') => {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    setLog(prev => [...prev, { time, msg, level }]);
  };

  useEffect(() => {
    (async () => {
      try {
        const r = await invoke('get_initial_paths');
        setWorkDir(r.workDir);
        setOutputDir(r.outputDir);
        setConfigDir(r.configDir);
      } catch (e) {
        addLog(`初始化失败: ${e}`, 'error');
      }
    })();
  }, []);

  const refreshCategory = useCallback(async () => {
    try {
      const cat = await invoke('read_category');
      setCategoryList(Array.isArray(cat) ? cat : []);
    } catch (e) {
      addLog(`读取报销类别失败: ${e}`, 'error');
    }
  }, []);
  useEffect(() => { refreshCategory(); }, [refreshCategory]);

  const pickDir = async (target) => {
    try {
      const p = await invoke('pick_directory', { title: target === 'work' ? '选择工作目录' : '选择输出目录' });
      if (p) {
        if (target === 'work') setWorkDir(p); else setOutputDir(p);
        addLog(`已选择${target === 'work' ? '工作' : '输出'}目录: ${p}`, 'success');
      }
    } catch (e) { addLog(`选择目录失败: ${e}`, 'error'); }
  };

  const openDir = async (kind) => {
    const path = kind === 'work' ? workDir : outputDir;
    if (!path) { showToast('请先选择目录', 'error'); return; }
    try { await invoke('open_directory', { path }); }
    catch (e) { showToast(`打开失败: ${e}`, 'error'); }
  };

  const importInvoices = async () => {
    if (!workDir) { showToast('请先选择工作目录', 'error'); return; }
    try {
      const files = await invoke('list_pdfs', { path: workDir });
      if (!files.length) { showToast('工作目录内无 PDF 文件', 'error'); addLog('工作目录内未发现 PDF', 'warn'); return; }
      addLog(`开始导入，共 ${files.length} 个 PDF`, 'info');
      const rows = await invoke('import_invoices', { paths: files });
      setInvoices(rows);
      const ok = rows.filter(r => r.is_invoice_pdf).length;
      const skip = rows.length - ok;
      // 逐文件日志
      for (const r of rows) {
        if (r.error) addLog(`${r.file_name}: ${r.error}`, 'warn');
      }
      addLog(`导入完成: 识别 ${ok} 张发票, 跳过 ${skip} 个非发票文件`, 'success');
      showToast(`导入完成(识别 ${ok}/跳过 ${skip})`, 'success');
    } catch (e) {
      showToast('导入失败', 'error');
      addLog(`导入失败: ${e}`, 'error');
    }
  };

  const clearImported = () => {
    if (!invoices.length) { showToast('当前没有导入的发票', 'info'); return; }
    setInvoices([]);
    setFilterMode(FILTER_ALL);
    setProjectFilter('');
    addLog('已清空导入结果', 'info');
  };

  const projectOptions = useMemo(() => {
    const s = new Set();
    for (const r of invoices) if (r.project_name) s.add(r.project_name);
    return Array.from(s).sort();
  }, [invoices]);

  const filtered = useMemo(() => {
    let rows = invoices;
    if (filterMode === FILTER_OK) rows = rows.filter(r => r.category && r.category.trim());
    else if (filterMode === FILTER_NONE) rows = rows.filter(r => !r.category || !r.category.trim());
    if (projectFilter) rows = rows.filter(r => r.project_name === projectFilter);
    return rows.map((r, i) => ({ ...r, _idx: i + 1 }));
  }, [invoices, filterMode, projectFilter]);

  const applyFilter = () => {
    if (filterMode === FILTER_OK) addLog('筛选: 可报销', 'info');
    else if (filterMode === FILTER_NONE) addLog('筛选: 不可报销', 'info');
    else addLog('筛选: 全部', 'info');
    if (projectFilter) addLog(`项目名称: ${projectFilter}`, 'info');
  };

  const toggleSelect = (file) => {
    setInvoices(prev => prev.map(r => r.file === file ? { ...r, _selected: !r._selected } : r));
  };
  const toggleSelectAll = (val) => {
    setInvoices(prev => prev.map(r => ({ ...r, _selected: val })));
  };

  const setCategoryOne = (file, cat) => {
    setInvoices(prev => prev.map(r => r.file === file ? { ...r, category: cat } : r));
  };

  const bulkSetCategory = () => {
    const targets = invoices.filter(r => r._selected);
    if (!targets.length) { showToast('请先勾选要设置的发票(序号前复选框)', 'error'); return; }
    const realCat = bulkCategory === '__CLEAR__' ? '' : bulkCategory;
    setInvoices(prev => prev.map(r => r._selected ? { ...r, category: realCat } : r));
    const label = realCat || '空';
    addLog(`已对 ${targets.length} 张发票设置类别: ${label}`, 'success');
    showToast(`已设置 ${targets.length} 张`, 'success');
  };

  const exportLedger = () => showToast('费用台账输出功能待模板确认后启用', 'info');
  const exportCover = () => showToast('报销封面输出功能待模板确认后启用', 'info');

  const mergePdfs = async () => {
    const targets = invoices.filter(r => r.is_invoice_pdf);
    if (!targets.length) { showToast('当前没有可合并的发票', 'error'); return; }
    if (!outputDir) { showToast('请先选择输出目录', 'error'); return; }
    try {
      addLog(`开始合并 PDF: ${targets.length} 张`, 'info');
      const r = await invoke('merge_pdfs', { inputFiles: targets.map(t => t.file), outputDir, filePrefix: '汇总发票' });
      addLog(`合并完成,共生成 ${r.total} 个汇总文件`, 'success');
      showToast(`合并完成: ${r.total} 个文件`, 'success');
    } catch (e) {
      showToast(`合并失败: ${e}`, 'error');
      addLog(`合并失败: ${e}`, 'error');
    }
  };

  const importMapping = async () => {
    try {
      const p = await invoke('pick_directory');
      // 实际用 file dialog pick_file, 这里改用 ask
    } catch (e) {}
    try {
      const src = await window.__TAURI__.dialog.open({
        title: '选择要导入的 mapping.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });
      if (!src) return;
      const path = typeof src === 'string' ? src : src.path;
      await invoke('import_mapping', { src: path });
      showToast('导入成功', 'success');
      addLog(`导入报销类别成功: ${path}`, 'success');
    } catch (e) {
      showToast('导入失败', 'error');
      addLog(`导入报销类别失败`, 'error');
    }
  };

  const exportMapping = async () => {
    try {
      const dest = await window.__TAURI__.dialog.save({
        title: '导出 mapping.json',
        defaultPath: 'mapping.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!dest) return;
      const path = typeof dest === 'string' ? dest : dest.path;
      await invoke('export_mapping', { dest: path });
      showToast('导出成功', 'success');
      addLog(`已导出 mapping.json: ${path}`, 'success');
    } catch (e) {
      showToast(`导出失败: ${e}`, 'error');
      addLog(`导出失败: ${e}`, 'error');
    }
  };

  const clearLog = () => { setLog([]); addLog('日志已清空', 'info'); };

  return (
    <div class="app-main">
      <div class="panel">
        <div class="panel-title">📁 发票助手</div>
        <div class="dir-row">
          <label>工作目录</label>
          <div class="input-row">
            <input type="text" value={workDir} onInput={e => setWorkDir(e.currentTarget.value)} placeholder="请选择工作目录" />
          </div>
          <button class="btn-browse primary" onClick={() => pickDir('work')}>浏览</button>
        </div>
        <div class="dir-row">
          <label>输出目录</label>
          <div class="input-row">
            <input type="text" value={outputDir} onInput={e => setOutputDir(e.currentTarget.value)} placeholder="请选择输出目录" />
          </div>
          <button class="btn-browse primary" onClick={() => pickDir('output')}>浏览</button>
        </div>
      </div>

      <div class="panel">
        <div class="btn-row">
          <button class="btn-action success" onClick={() => openDir('work')}>打开工作目录</button>
          <button class="btn-action success" onClick={() => openDir('output')}>打开输出目录</button>
          <button class="btn-action" onClick={importInvoices} disabled={!workDir}>发票导入</button>
          <button class="btn-action warn" onClick={clearImported}>清空导入</button>
        </div>
      </div>

      <div class="panel">
        <div class="btn-row">
          <button class="btn-action" onClick={() => setShowConfig('view')}>查看报销类别</button>
          <button class="btn-action" onClick={importMapping}>导入报销类别</button>
          <button class="btn-action" onClick={exportMapping}>导出报销类别</button>
          <button class="btn-action" onClick={() => setShowConfig('edit')}>修改报销类别</button>
        </div>
      </div>

      <div class="panel" style="flex:1; min-height:280px; display:flex; flex-direction:column;">
        <div class="panel-title">导入结果</div>
        <div class="filter-row">
          <button class={'btn-action' + (filterMode === FILTER_ALL ? ' success' : '')} onClick={() => setFilterMode(FILTER_ALL)}>全部</button>
          <button class={'btn-action' + (filterMode === FILTER_OK ? ' success' : '')} onClick={() => setFilterMode(FILTER_OK)}>可报销</button>
          <button class={'btn-action' + (filterMode === FILTER_NONE ? ' success' : '')} onClick={() => setFilterMode(FILTER_NONE)}>不可报销</button>
          <select value={projectFilter} onChange={e => setProjectFilter(e.currentTarget.value)}>
            <option value="">项目名称(全部)</option>
            {projectOptions.map(p => <option value={p}>{p}</option>)}
          </select>
          <button class="btn-action" onClick={applyFilter}>筛选项目名称</button>
          <select value={bulkCategory} onChange={e => setBulkCategory(e.currentTarget.value)}>
            <option value="">报销类别(下拉选择)</option>
            <option value="__CLEAR__">（清空）</option>
            {categoryList.map(c => <option value={c}>{c}</option>)}
          </select>
          <button class="btn-action" onClick={bulkSetCategory}>一键设置类别</button>
        </div>

        <div class="table-wrap">
          <table class="invoice-table">
            <thead>
              <tr>
                <th class="check-cell">
                  <input type="checkbox" class="checkbox"
                    checked={invoices.length > 0 && invoices.every(r => r._selected)}
                    onChange={e => toggleSelectAll(e.currentTarget.checked)} />
                </th>
                <th class="no-cell">序号</th>
                <th>发票类别</th>
                <th>开票日期</th>
                <th>发票号码</th>
                <th class="amount">发票金额</th>
                <th class="text">购买方</th>
                <th class="text">项目名称</th>
                <th>报销类别</th>
              </tr>
            </thead>
            <tbody>
              {filtered.length === 0 && (
                <tr><td colSpan="9" style="padding:24px;color:var(--text-muted);">暂无数据,请先点击「发票导入」</td></tr>
              )}
              {filtered.map((r) => (
                <tr key={r.file}>
                  <td class="check-cell">
                    <input type="checkbox" class="checkbox"
                      checked={!!r._selected}
                      onChange={() => toggleSelect(r.file)} />
                  </td>
                  <td class="no-cell">{r._idx}</td>
                  <td>{r.invoice_type || (r.is_invoice_pdf ? '-' : '非发票')}</td>
                  <td>{r.issue_date || '-'}</td>
                  <td>{r.invoice_no || '-'}</td>
                  <td class="amount">{r.amount ? `¥ ${r.amount}` : '-'}</td>
                  <td class="text">{r.buyer || '-'}</td>
                  <td class="text">{r.project_name || '-'}</td>
                  <td>
                    <select value={r.category || ''} onChange={e => setCategoryOne(r.file, e.currentTarget.value)} style="min-width:110px">
                      <option value="">(空)</option>
                      {categoryList.map(c => <option value={c}>{c}</option>)}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div class="panel">
        <div class="btn-row">
          <button class="btn-action" onClick={exportLedger}>输出费用台账</button>
          <button class="btn-action" onClick={exportCover}>输出报销封面</button>
          <button class="btn-action warn" onClick={mergePdfs}>输出汇总PDF</button>
        </div>
      </div>

      <div class="panel">
        <div class="btn-row">
          <div style="font-weight:700;color:var(--primary-dark);">处理日志</div>
          <div class="flex-spacer" />
          <button class="btn-action danger" onClick={clearLog}>清空</button>
        </div>
        <div class="log-wrap">
          {log.length === 0 && <div class="log-line info">等待操作...</div>}
          {log.map((l, i) => (
            <div key={i} class={`log-line ${l.level}`}>[{l.time}] {l.msg}</div>
          ))}
        </div>
      </div>

      {showConfig && (
        <ConfigView mode={showConfig}
          onClose={() => { setShowConfig(false); refreshCategory(); }}
          addLog={addLog} showToast={showToast}
          categoryList={categoryList} />
      )}

      {toast && <div class={`toast ${toast.type}`}>{toast.msg}</div>}
    </div>
  );
}
