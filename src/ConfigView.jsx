import { useEffect, useState } from 'preact/hooks';
import { invoke } from './bridge.js';

const REQUIRED_KEYS = ['项目名称', '通用项目名称', '大类别', '报销类别'];

export function ConfigView({ mode, onClose, addLog, showToast, categoryList }) {
  const isEdit = mode === 'edit';
  const [rows, setRows] = useState([]);
  const [editing, setEditing] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const data = await invoke('read_mapping');
        setRows(Array.isArray(data) ? data : []);
      } catch (e) { addLog(`读取 mapping.json 失败: ${e}`, 'error'); }
    })();
  }, []);

  const updateRow = (idx, key, val) => {
    setRows(prev => prev.map((r, i) => i === idx ? { ...r, [key]: val } : r));
    setDirty(true);
  };
  const addRow = () => {
    setRows(prev => [...prev, { 项目名称: '', 通用项目名称: '', 大类别: '', 报销类别: '' }]);
    setEditing(true); setDirty(true);
  };
  const removeRow = (idx) => {
    setRows(prev => prev.filter((_, i) => i !== idx));
    setDirty(true);
  };
  const saveAll = async () => {
    try {
      await invoke('write_mapping', { data: rows });
      setDirty(false);
      addLog('mapping.json 已保存', 'success');
      showToast('保存成功', 'success');
    } catch (e) { showToast(`保存失败: ${e}`, 'error'); }
  };
  const discard = () => {
    if (!confirm('确定放弃所有未保存的修改?')) return;
    setEditing(false);
    setDirty(false);
    (async () => {
      const data = await invoke('read_mapping');
      setRows(Array.isArray(data) ? data : []);
    })();
  };

  return (
    <div class="modal-mask" onClick={e => { if (e.target === e.currentTarget) onClose(); }}>
      <div class="modal-box">
        <div class="modal-head">
          <h3>报销类别映射{isEdit ? '修改器' : '查看器'}</h3>
          <div style="display:flex;align-items:center;gap:10px;">
            <span style="color:var(--text-secondary);font-size:14px;">
              {isEdit ? (dirty ? '● 有未保存修改' : '当前为编辑模式') : '当前为只读模式'}
            </span>
            <button class="btn-action" onClick={onClose}>关闭</button>
          </div>
        </div>
        <div class="modal-body">
          <table class="config-table">
            <thead>
              <tr>
                <th style="width:50px;">序号</th>
                <th>项目名称</th>
                <th>通用项目名称</th>
                <th>大类别</th>
                <th>报销类别</th>
                {isEdit && <th style="width:80px;">操作</th>}
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 && (
                <tr><td colSpan={isEdit ? 6 : 5} style="padding:24px;color:var(--text-muted);">
                  {isEdit ? '尚无映射规则,点击下方"新增映射规则"开始添加' : '尚无数据'}
                </td></tr>
              )}
              {rows.map((r, i) => (
                <tr key={i}>
                  <td>{i + 1}</td>
                  {isEdit ? (
                    <>
                      <td><input value={r['项目名称'] || ''} onInput={e => updateRow(i, '项目名称', e.currentTarget.value)} placeholder="如 *住宿*" /></td>
                      <td><input value={r['通用项目名称'] || ''} onInput={e => updateRow(i, '通用项目名称', e.currentTarget.value)} /></td>
                      <td><input value={r['大类别'] || ''} onInput={e => updateRow(i, '大类别', e.currentTarget.value)} /></td>
                      <td>
                        <select value={r['报销类别'] || ''} onChange={e => updateRow(i, '报销类别', e.currentTarget.value)}>
                          <option value="">(请选择)</option>
                          {categoryList.map(c => <option value={c}>{c}</option>)}
                        </select>
                      </td>
                      <td><button class="btn-action danger" onClick={() => removeRow(i)}>删除</button></td>
                    </>
                  ) : (
                    <>
                      <td>{r['项目名称'] || '-'}</td>
                      <td>{r['通用项目名称'] || '-'}</td>
                      <td>{r['大类别'] || '-'}</td>
                      <td>{r['报销类别'] || '-'}</td>
                    </>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {isEdit && (
          <div class="modal-foot">
            <button class="btn-action success" onClick={addRow}>新增映射规则</button>
            <button class="btn-action" onClick={saveAll} disabled={!dirty}>保存修改</button>
            <button class="btn-action danger" onClick={discard} disabled={!dirty}>放弃修改</button>
          </div>
        )}
      </div>
    </div>
  );
}
