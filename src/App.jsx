import { useEffect, useMemo, useState, useCallback } from 'preact/hooks';
import { invoke } from './bridge.js';
import { open as dialogOpen, save as dialogSave } from '@tauri-apps/plugin-dialog';
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
  const [invoiceNoFilter, setInvoiceNoFilter] = useState('');
  const [buyerFilter, setBuyerFilter] = useState('');
  const [ownerFilter, setOwnerFilter] = useState('');
  const [categoryFilter, setCategoryFilter] = useState('');
  const [categoryList, setCategoryList] = useState([]);
  const [log, setLog] = useState([]);
  const [toast, setToast] = useState(null);
  const [showConfig, setShowConfig] = useState(false);
  const [mappingPath, setMappingPath] = useState('');
  const [showInfoDetail, setShowInfoDetail] = useState(false);
  const [contextMenu, setContextMenu] = useState(null);

  const showToast = (msg, type = 'info') => {
    setToast({ msg, type });
    setTimeout(() => setToast(null), 2200);
  };
  const addLog = (msg, level = 'info', detail = false) => {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    setLog(prev => [...prev, { time, msg, level, detail }]);
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
      const entries = await invoke('list_pdfs', { path: workDir });
      if (!entries.length) { showToast('工作目录内无 PDF 文件', 'error'); addLog('工作目录内未发现 PDF', 'warn'); return; }
      addLog(`开始导入，共 ${entries.length} 个 PDF`, 'info');
      const rows = await invoke('import_invoices', { entries });
      // 去重: 检查发票号唯一性
      const existingInvoiceNos = new Set(invoices.filter(r => r.invoice_no).map(r => r.invoice_no));
      const skippedDuplicates = [];
      const newRows = [];
      for (const r of rows) {
        if (r.is_invoice_pdf && r.invoice_no && existingInvoiceNos.has(r.invoice_no)) {
          skippedDuplicates.push({ file: r.file_name, invoice_no: r.invoice_no });
          continue;
        }
        if (r.is_invoice_pdf && r.invoice_no) {
          existingInvoiceNos.add(r.invoice_no);
        }
        newRows.push(r);
      }
      setInvoices(prev => [...prev, ...newRows]);
      const ok = newRows.filter(r => r.is_invoice_pdf).length;
      const skip = rows.length - ok - skippedDuplicates.length;
      // 逐文件日志
      for (const r of newRows) {
        if (r.error && !r.is_invoice_pdf) {
          addLog(`${r.file_name}: ${r.error}`, 'warn');
        } else {
          const json = JSON.stringify({
            file: r.file_name,
            type: r.invoice_type,
            date: r.issue_date,
            no: r.invoice_no,
            amount: r.amount,
            buyer: r.buyer,
            project: r.project_name,
          });
          addLog(json, 'info', true);
        }
      }
      // 重复发票提示
      if (skippedDuplicates.length > 0) {
        const dupFiles = skippedDuplicates.map(d => d.file).join(', ');
        addLog(`跳过重复发票: ${dupFiles}`, 'warn');
      }
      addLog(`导入完成: 新增 ${ok} 张发票, 跳过 ${skip} 个非发票, ${skippedDuplicates.length} 个重复`, 'success');
      showToast(`导入完成(新增${ok}/跳过非发票${skip}/重复${skippedDuplicates.length})`, 'success');
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
    setInvoiceNoFilter('');
    setBuyerFilter('');
    setOwnerFilter('');
    setCategoryFilter('');
    addLog('已清空导入结果', 'info');
  };

  // 动态筛选：基础过滤（不包含四个下拉框的筛选）
  const baseFiltered = useMemo(() => {
    let rows = invoices.filter(r => r.is_invoice_pdf);
    if (filterMode === FILTER_OK) rows = rows.filter(r => r.category && r.category.trim());
    else if (filterMode === FILTER_NONE) rows = rows.filter(r => !r.category || !r.category.trim());
    if (invoiceNoFilter.trim()) rows = rows.filter(r => r.invoice_no && r.invoice_no.includes(invoiceNoFilter.trim()));
    return rows;
  }, [invoices, filterMode, invoiceNoFilter]);

  // 动态选项：根据其他筛选条件计算每个下拉框的可选值
  const ownerOptions = useMemo(() => {
    let rows = baseFiltered;
    if (buyerFilter) rows = rows.filter(r => r.buyer === buyerFilter);
    if (projectFilter) rows = rows.filter(r => r.project_name === projectFilter);
    if (categoryFilter === '__UNMATCHED__') rows = rows.filter(r => !r.category || !r.category.trim());
    else if (categoryFilter) rows = rows.filter(r => r.category === categoryFilter);
    const s = new Set();
    for (const r of rows) if (r.owner) s.add(r.owner);
    return Array.from(s).sort();
  }, [baseFiltered, buyerFilter, projectFilter, categoryFilter]);

  const buyerOptions = useMemo(() => {
    let rows = baseFiltered;
    if (ownerFilter) rows = rows.filter(r => r.owner === ownerFilter);
    if (projectFilter) rows = rows.filter(r => r.project_name === projectFilter);
    if (categoryFilter === '__UNMATCHED__') rows = rows.filter(r => !r.category || !r.category.trim());
    else if (categoryFilter) rows = rows.filter(r => r.category === categoryFilter);
    const s = new Set();
    for (const r of rows) if (r.buyer) s.add(r.buyer);
    return Array.from(s).sort();
  }, [baseFiltered, ownerFilter, projectFilter, categoryFilter]);

  const projectOptions = useMemo(() => {
    let rows = baseFiltered;
    if (ownerFilter) rows = rows.filter(r => r.owner === ownerFilter);
    if (buyerFilter) rows = rows.filter(r => r.buyer === buyerFilter);
    if (categoryFilter === '__UNMATCHED__') rows = rows.filter(r => !r.category || !r.category.trim());
    else if (categoryFilter) rows = rows.filter(r => r.category === categoryFilter);
    const s = new Set();
    for (const r of rows) if (r.project_name) s.add(r.project_name);
    return Array.from(s).sort();
  }, [baseFiltered, ownerFilter, buyerFilter, categoryFilter]);

  const categoryOptions = useMemo(() => {
    let rows = baseFiltered;
    if (ownerFilter) rows = rows.filter(r => r.owner === ownerFilter);
    if (buyerFilter) rows = rows.filter(r => r.buyer === buyerFilter);
    if (projectFilter) rows = rows.filter(r => r.project_name === projectFilter);
    const s = new Set();
    for (const r of rows) if (r.category) s.add(r.category);
    return Array.from(s).sort();
  }, [baseFiltered, ownerFilter, buyerFilter, projectFilter]);

  const hasUnmatched = useMemo(() => {
    return baseFiltered.some(r => !r.category || !r.category.trim());
  }, [baseFiltered]);

  // 最终筛选结果
  const filtered = useMemo(() => {
    let rows = baseFiltered;
    if (ownerFilter) rows = rows.filter(r => r.owner === ownerFilter);
    if (buyerFilter) rows = rows.filter(r => r.buyer === buyerFilter);
    if (projectFilter) rows = rows.filter(r => r.project_name === projectFilter);
    if (categoryFilter === '__UNMATCHED__') rows = rows.filter(r => !r.category || !r.category.trim());
    else if (categoryFilter) rows = rows.filter(r => r.category === categoryFilter);
    return rows.map((r, i) => ({ ...r, _idx: i + 1 }));
  }, [baseFiltered, ownerFilter, buyerFilter, projectFilter, categoryFilter]);

  const toggleSelect = (file) => {
    setInvoices(prev => prev.map(r => r.file === file ? { ...r, _selected: !r._selected } : r));
  };
  const toggleSelectAll = (val) => {
    const filteredFiles = new Set(filtered.map(r => r.file));
    setInvoices(prev => prev.map(r => filteredFiles.has(r.file) ? { ...r, _selected: val } : r));
  };

  const exportLedger = async () => {
    const selected = invoices.filter(r => r._selected && r.is_invoice_pdf);
    if (!selected.length) { showToast('请先勾选要生成费用台账的发票', 'error'); return; }
    if (!outputDir) { showToast('请先选择输出目录', 'error'); return; }
    try {
      addLog(`开始生成费用台账: ${selected.length} 张发票`, 'info');
      const result = await invoke('generate_ledger_pdf', { invoices: selected, outputDir });
      addLog(`费用台账生成完成,共生成 ${result.total} 个文件`, 'success');
      showToast(`费用台账生成完成: ${result.total} 个文件`, 'success');
    } catch (e) {
      showToast(`费用台账生成失败: ${e}`, 'error');
      addLog(`费用台账生成失败: ${e}`, 'error');
    }
  };
  
  const [coverOutputFormat, setCoverOutputFormat] = useState('pdf'); // 'xlsx', 'pdf', 'both'

  const exportCover = async () => {
    const selected = invoices.filter(r => r._selected && r.is_invoice_pdf);
    if (!selected.length) { showToast('请先勾选要生成报销封面的发票', 'error'); return; }
    if (!outputDir) { showToast('请先选择输出目录', 'error'); return; }
    try {
      addLog(`开始生成报销封面: ${selected.length} 张发票`, 'info');
      const result = await invoke('generate_cover_pdf', { invoices: selected, outputDir, outputFormat: coverOutputFormat });
      addLog(`报销封面生成完成,共生成 ${result.total} 个文件`, 'success');
      showToast(`报销封面生成完成: ${result.total} 个文件`, 'success');
    } catch (e) {
      showToast(`报销封面生成失败: ${e}`, 'error');
      addLog(`报销封面生成失败: ${e}`, 'error');
    }
  };

  const matchCategories = async () => {
    const unmatched = invoices.filter(r => r.is_invoice_pdf && (!r.category || !r.category.trim()));
    if (!unmatched.length) { showToast('没有未匹配报销类别的发票', 'info'); return; }
    try {
      const mappings = await invoke('read_mapping');
      if (!Array.isArray(mappings) || !mappings.length) { showToast('mapping.json 为空，请先维护映射规则', 'error'); return; }
      addLog(`加载 mapping: ${mappings.length} 条规则`, 'info', true);
      addLog(`待匹配发票: ${unmatched.length} 张`, 'info', true);
      let matchedCount = 0;
      const updated = invoices.map(r => {
        if (!r.is_invoice_pdf || (r.category && r.category.trim())) return r;
        const cleanProjectName = (r.project_name || '').replace(/\*/g, '').trim();
        if (!cleanProjectName) return r;
        for (const m of mappings) {
          const pattern = m['项目名称'] || '';
          const category = m['报销类别'] || '';
          if (!pattern || !category) continue;
          const parts = pattern.split('|');
          for (const part of parts) {
            const cleaned = part.replace(/\*/g, '').trim();
            if (!cleaned) continue;
            if (cleanProjectName.includes(cleaned)) {
              matchedCount++;
              addLog(`匹配成功: ${r.file_name} [${r.project_name}] -> ${category}`, 'info', true);
              return { ...r, category };
            }
          }
        }
        return r;
      });
      setInvoices(updated);
      addLog(`匹配完成: ${matchedCount} 张发票匹配成功`, 'success');
      showToast(`匹配完成: ${matchedCount} 张`, 'success');
    } catch (e) {
      showToast(`匹配失败: ${e}`, 'error');
      addLog(`匹配失败: ${e}`, 'error');
    }
  };

  const mergePdfs = async () => {
    const selected = invoices.filter(r => r._selected && r.is_invoice_pdf);
    if (!selected.length) { showToast('请先勾选要合并的发票', 'error'); return; }
    if (!outputDir) { showToast('请先选择输出目录', 'error'); return; }
    try {
      // 创建汇总PDF子目录
      const mergeDir = `${outputDir}/汇总PDF`;
      // 按报销人+购买方分组
      const groups = {};
      for (const r of selected) {
        const owner = r.owner || '未分组';
        const buyer = r.buyer || '未知购买方';
        const key = `${owner}_${buyer}`;
        if (!groups[key]) groups[key] = { owner, buyer, files: [] };
        groups[key].files.push(r.file);
      }
      const keys = Object.keys(groups);
      addLog(`开始合并 PDF: ${selected.length} 张，按报销人+购买方分为 ${keys.length} 组`, 'info');
      let totalFiles = 0;
      for (const key of keys) {
        const { owner, buyer, files } = groups[key];
        const prefix = `合并发票_${owner}_${buyer}(${files.length}张)`;
        addLog(`合并 ${prefix}: ${files.length} 张`, 'info');
        const r = await invoke('merge_pdfs', { inputFiles: files, outputDir: mergeDir, filePrefix: prefix });
        totalFiles += r.total;
      }
      addLog(`合并完成,共生成 ${totalFiles} 个汇总文件`, 'success');
      showToast(`合并完成: ${totalFiles} 个文件`, 'success');
    } catch (e) {
      showToast(`合并失败: ${e}`, 'error');
      addLog(`合并失败: ${e}`, 'error');
    }
  };

  const importMapping = async () => {
    try {
      const src = await dialogOpen({
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
      addLog(`导入报销类别失败: ${e}`, 'error');
    }
  };

  const exportMapping = async () => {
    try {
      const dest = await dialogSave({
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

  const openConfig = async (mode) => {
    try {
      const path = await invoke('get_mapping_path');
      setMappingPath(path);
    } catch (e) { setMappingPath(''); }
    setShowConfig(mode);
  };

  const clearLog = () => { setLog([]); addLog('日志已清空', 'info'); };

  const handleContextMenu = (e, fileName) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, fileName });
  };

  const copyFileName = async (fileName) => {
    try {
      await navigator.clipboard.writeText(fileName);
      showToast('已复制文件名', 'success');
    } catch (e) {
      showToast('复制失败', 'error');
    }
    setContextMenu(null);
  };

  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    if (contextMenu) {
      document.addEventListener('click', handleClick);
      return () => document.removeEventListener('click', handleClick);
    }
  }, [contextMenu]);

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
          <button class="btn-action" onClick={() => openConfig('view')}>查看报销类别</button>
          <button class="btn-action" onClick={importMapping}>导入报销类别</button>
          <button class="btn-action" onClick={exportMapping}>导出报销类别</button>
          <button class="btn-action" onClick={() => openConfig('edit')}>修改报销类别</button>
          <button class="btn-action success" onClick={matchCategories}>匹配报销类别</button>
        </div>
      </div>

      <div class="panel" style="flex:1; min-height:280px; display:flex; flex-direction:column;">
        <div class="panel-title">导入结果</div>
        <div class="filter-row">
          <div class="filter-group">
            <div class="filter-buttons">
              <button class={'btn-action' + (filterMode === FILTER_ALL ? ' success' : '')} onClick={() => setFilterMode(FILTER_ALL)}>全部</button>
              <button class={'btn-action' + (filterMode === FILTER_OK ? ' success' : '')} onClick={() => setFilterMode(FILTER_OK)}>可报销</button>
              <button class={'btn-action' + (filterMode === FILTER_NONE ? ' success' : '')} onClick={() => setFilterMode(FILTER_NONE)}>不可报销</button>
            </div>
            <input type="text" value={invoiceNoFilter} onInput={e => setInvoiceNoFilter(e.currentTarget.value)} placeholder="发票号码" class="invoice-no-input" />
          </div>
          <div class="filter-group">
            <select value={ownerFilter} onChange={e => setOwnerFilter(e.currentTarget.value)}>
              <option value="">报销人(全部)</option>
              {ownerOptions.map(o => <option value={o}>{o}</option>)}
            </select>
            <select value={buyerFilter} onChange={e => setBuyerFilter(e.currentTarget.value)}>
              <option value="">购买方(全部)</option>
              {buyerOptions.map(b => <option value={b}>{b}</option>)}
            </select>
          </div>
          <div class="filter-group">
            <select value={projectFilter} onChange={e => setProjectFilter(e.currentTarget.value)}>
              <option value="">项目名称(全部)</option>
              {projectOptions.map(p => <option value={p}>{p}</option>)}
            </select>
            <select value={categoryFilter} onChange={e => setCategoryFilter(e.currentTarget.value)}>
              <option value="">报销类别(全部)</option>
              {hasUnmatched && <option value="__UNMATCHED__">报销类别(未匹配)</option>}
              {categoryOptions.map(c => <option value={c}>{c}</option>)}
            </select>
          </div>
        </div>

        <div class="table-wrap">
          <table class="invoice-table">
            <thead>
              <tr>
                <th class="check-cell">
                  <input type="checkbox" class="checkbox"
                    checked={filtered.length > 0 && filtered.every(r => r._selected)}
                    onChange={e => toggleSelectAll(e.currentTarget.checked)} />
                </th>
                <th class="no-cell">序号</th>
                <th class="text">文件名</th>
                <th>报销人</th>
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
                <tr><td colSpan="10" style="padding:24px;color:var(--text-muted);">暂无数据,请先点击「发票导入」</td></tr>
              )}
              {filtered.map((r) => (
                <tr key={r.file}>
                  <td class="check-cell">
                    <input type="checkbox" class="checkbox"
                      checked={!!r._selected}
                      onChange={() => toggleSelect(r.file)} />
                  </td>
                  <td class="no-cell">{r._idx}</td>
                  <td class="text file-name" title={r.file_name} onContextMenu={e => handleContextMenu(e, r.file_name)}>{r.file_name}</td>
                  <td>{r.owner || '-'}</td>
                  <td>{r.invoice_type || (r.is_invoice_pdf ? '-' : '非发票')}</td>
                  <td>{r.issue_date || '-'}</td>
                  <td>{r.invoice_no || '-'}</td>
                  <td class="amount">{r.amount ? `¥ ${r.amount}` : '-'}</td>
                  <td class="text">{r.buyer || '-'}</td>
                  <td class="text">{r.project_name || '-'}</td>
                  <td>{r.category || '(未匹配)'}</td>
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
          <select class="btn-select" value={coverOutputFormat} onChange={e => setCoverOutputFormat(e.currentTarget.value)}>
            <option value="pdf">仅 PDF</option>
            <option value="xlsx">仅 xlsx</option>
            <option value="both">xlsx + PDF</option>
          </select>
          <button class="btn-action warn" onClick={mergePdfs}>输出汇总PDF</button>
        </div>
      </div>

      <div class="panel">
        <div class="btn-row">
          <div style="font-weight:700;color:var(--primary-dark);">处理日志</div>
          <div class="flex-spacer" />
          <button class={'btn-action' + (showInfoDetail ? ' success' : '')} onClick={() => setShowInfoDetail(!showInfoDetail)}>
            Info {showInfoDetail ? 'ON' : 'OFF'}
          </button>
          <button class="btn-action danger" onClick={clearLog}>清空</button>
        </div>
        <div class="log-wrap">
          {log.length === 0 && <div class="log-line info">等待操作...</div>}
          {log.filter(l => showInfoDetail || !l.detail).map((l, i) => (
            <div key={i} class={`log-line ${l.level}`}>[{l.time}] {l.msg}</div>
          ))}
        </div>
      </div>

      {showConfig && (
        <ConfigView mode={showConfig}
          onClose={() => { setShowConfig(false); refreshCategory(); }}
          addLog={addLog} showToast={showToast}
          categoryList={categoryList} configPath={mappingPath} />
      )}

      {toast && <div class={`toast ${toast.type}`}>{toast.msg}</div>}

      {contextMenu && (
        <div class="context-menu" style={`left:${contextMenu.x}px;top:${contextMenu.y}px`}>
          <div class="context-menu-item" onClick={() => copyFileName(contextMenu.fileName)}>复制文件名</div>
        </div>
      )}
    </div>
  );
}
