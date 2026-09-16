import assert from 'node:assert/strict'
import test from 'node:test'
import {
  DIR_ENTRY, FILE_ENTRY, acceptEntries, accessLabel, countLabel, isReadOnly, kindIcon,
  kindLabel, pickFilesOnly,
} from './workspace.js'

const dir = { id: 'w1', kind: DIR_ENTRY, path: 'C:\\proj', label: 'proj' }
const file = { id: 'w2', kind: FILE_ENTRY, path: 'C:\\proj\\report.pdf', label: 'report.pdf' }

test('目录条目可读写，文件条目永远只读', () => {
  assert.equal(isReadOnly(dir), false)
  assert.equal(isReadOnly(file), true)
  assert.equal(accessLabel(dir), '可读写')
  assert.equal(accessLabel(file), '只读')
  assert.equal(kindLabel(dir), '文件夹')
  assert.equal(kindLabel(file), '文件')
  assert.equal(kindIcon(dir), 'folder')
  assert.equal(kindIcon(file), 'file')
})

test('不认识的条目按只读处理——宁可在界面上多说一句，也不假装能写', () => {
  assert.equal(isReadOnly({ kind: '别的' }), true)
  assert.equal(isReadOnly(undefined), true)
})

test('条目数为空时说清下一步，不为空时报个数', () => {
  assert.equal(countLabel([]), '还没有授权任何文件')
  assert.equal(countLabel([dir, file]), '已授权 2 条')
  assert.equal(countLabel(undefined), '还没有授权任何文件')
})

test('只接受数组形式的快照，坏数据不覆盖现有清单', () => {
  const current = [dir]
  assert.equal(acceptEntries(current, undefined), current)
  assert.equal(acceptEntries(current, null), current)
  assert.equal(acceptEntries(current, { entries: [] }), current)
  assert.deepEqual(acceptEntries(current, [dir, file]), [dir, file])
  assert.deepEqual(acceptEntries(current, []), [], '空清单是合法状态（用户删光了）')
})

test('拖拽路径先过一遍，空字符串不往 Rust 送', () => {
  assert.deepEqual(pickFilesOnly(['C:\\a.txt', '  ', '', 'C:\\b.txt']), ['C:\\a.txt', 'C:\\b.txt'])
  assert.deepEqual(pickFilesOnly(undefined), [])
})
