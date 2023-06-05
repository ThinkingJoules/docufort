




struct DocSummary{
    doc_id: DocID,
    branch:Option<DocID>, //delta updates can have suffixes, doc_id is the 'root' this DocID is for the head of this version
    hist_clean: bool, //if any delta updates are in corrupted blocks
    corruption:Vec<u64>, //messages contained in corrupted blocks.
    genesis:Result<u64,(u64,[u8;32])>, //where the doc 'create' or 'archive' message is located

}
type SitRep = HashMap<DocID,DocSummary>;

struct BlockSummary{
    start_time:u64,
    best_effort:bool,
    messages:u32,
    len: u32,
    verified:bool,
    end_time:u64,
}
type Blocks = HashMap<u64,BlockSummary>;