use std::collections::*;
use std::rc::Rc;
use std::cell::RefCell;

use crate::*;
use crate::parser::*;
use crate::rule::*;
use crate::tree::*;

use rustnutlib::*;
use rustnutlib::console::*;

macro_rules! block_map {
    ($($block_name:expr => $func_name:ident), *,) => {
        {
            let mut block_map = BlockMap::new();
            $(block_map.insert($block_name.to_string(), Box::new(FCPEGBlock::$func_name()));)*
            block_map
        }
    };
}

macro_rules! block {
    ($block_name:expr, $cmds:expr) => {
        Block::new($block_name.to_string(), $cmds)
    };
}

macro_rules! rule {
    ($rule_name:expr, $($choice:expr), *,) => {
        {
            let sub_elems = vec![$(
                match $choice {
                    RuleElement::Group(_) => $choice,
                    _ => panic!(),
                }
            )*];

            let mut root_group = Box::new(RuleGroup::new(RuleGroupKind::Choice));
            root_group.sub_elems = sub_elems;
            root_group.ast_reflection_style = ASTReflectionStyle::Expansion;

            let rule = Rule::new(CharacterPosition::get_empty(), $rule_name.to_string(), String::new(), vec![], vec![], root_group);
            BlockCommand::Define { pos: CharacterPosition::get_empty(), rule: rule }
        }
    };
}

macro_rules! start_cmd {
    ($file_alias_name:expr, $block_name:expr, $rule_name:expr) => {
        BlockCommand::Start { pos: CharacterPosition::get_empty(), file_alias_name: $file_alias_name.to_string(), block_name: $block_name.to_string(), rule_name: $rule_name.to_string() }
    };
}

macro_rules! choice {
    ($options:expr, $($sub_elem:expr), *,) => {
        {
            let mut group = RuleGroup::new(RuleGroupKind::Sequence);
            group.sub_elems = vec![$($sub_elem,)*];
            group.ast_reflection_style = ASTReflectionStyle::Reflection(String::new());

            for opt in $options {
                match opt {
                    "&" | "!" => group.lookahead_kind = RuleElementLookaheadKind::new(opt),
                    "?" | "*" | "+" => group.loop_count = RuleElementLoopCount::from_symbol(opt),
                    "#" => group.ast_reflection_style = ASTReflectionStyle::NoReflection,
                    "##" => group.ast_reflection_style = ASTReflectionStyle::Expansion,
                    ":" => group.kind = RuleGroupKind::Choice,
                    _ => panic!(),
                }
            }

            RuleElement::Group(Box::new(group))
        }
    };
}

macro_rules! expr {
    ($kind:ident, $value:expr $(, $option:expr) *) => {
        {
            let mut expr = RuleExpression::new(CharacterPosition::get_empty(), RuleExpressionKind::$kind, $value.to_string());

            let leaf_name = match RuleExpressionKind::$kind {
                RuleExpressionKind::ID => $value.to_string(),
                _ => String::new(),
            };

            expr.ast_reflection_style = ASTReflectionStyle::Reflection(leaf_name);

            $(
                match $option {
                    "&" | "!" => expr.lookahead_kind = RuleElementLookaheadKind::new($option),
                    "?" | "*" | "+" => expr.loop_count = RuleElementLoopCount::from_symbol($option),
                    "#" => expr.ast_reflection_style = ASTReflectionStyle::NoReflection,
                    "##" => expr.ast_reflection_style = ASTReflectionStyle::Expansion,
                    _ => panic!(),
                }
            )*

            RuleElement::Expression(Box::new(expr))
        }
    };
}

pub type BlockMap = HashMap<String, Box<Block>>;

pub enum BlockParseError {
    Unknown(),
    BlockAliasNotFound { pos: CharacterPosition, block_alias_name: String },
    AttemptToAccessPrivateItem { pos: CharacterPosition, item_id: String },
    DuplicatedBlockName { pos: CharacterPosition, block_name: String },
    DuplicatedFileAliasName { file_alias_name: String },
    DuplicatedArgumentID { pos: CharacterPosition, arg_id: String },
    DuplicatedRuleName { pos: CharacterPosition, rule_name: String },
    DuplicatedStartCommand { pos: CharacterPosition },
    InternalError { msg: String },
    InvalidID { pos: CharacterPosition, id: String },
    InvalidLoopCount { pos: CharacterPosition },
    MainBlockNotDefined {},
    NamingRuleViolation { pos: CharacterPosition, id: String },
    NoStartCommandInMainBlock {},
    StartCommandOutsideMainBlock { pos: CharacterPosition },
    UnknownEscapeSequenceCharacter { pos: CharacterPosition },
}

impl ConsoleLogger for BlockParseError {
    fn get_log(&self) -> ConsoleLog {
        match self {
            BlockParseError::Unknown() => log!(Error, "unknown error"),
            BlockParseError::BlockAliasNotFound { pos, block_alias_name } => log!(Error, &format!("block alias '{}' not found", block_alias_name), format!("at:\t{}", pos)),
            BlockParseError::AttemptToAccessPrivateItem { pos, item_id } => log!(Warning, "attempt to access private item", format!("at:\t{}", pos), format!("id:\t{}", item_id)),
            BlockParseError::DuplicatedBlockName { pos, block_name } => log!(Error, &format!("duplicated block name '{}'", block_name), format!("at:\t{}", pos)),
            BlockParseError::DuplicatedFileAliasName { file_alias_name } => log!(Error, &format!("duplicated file alias name '{}'", file_alias_name)),
            BlockParseError::DuplicatedArgumentID { pos, arg_id } => log!(Error, &format!("duplicated argument id '{}'", arg_id), format!("at:\t{}", pos)),
            BlockParseError::DuplicatedRuleName { pos, rule_name } => log!(Error, &format!("duplicated rule name '{}'", rule_name), format!("at:\t{}", pos)),
            BlockParseError::DuplicatedStartCommand { pos } => log!(Error, "duplicated start command", format!("at:\t{}", pos)),
            BlockParseError::InternalError { msg } => log!(Error, &format!("internal error: {}", msg)),
            BlockParseError::InvalidID { pos, id } => log!(Error, &format!("invalid id '{}'", id), format!("at:\t{}", pos)),
            BlockParseError::InvalidLoopCount { pos } => log!(Error, &format!("invalid loop count"), format!("at:\t{}", pos)),
            BlockParseError::MainBlockNotDefined {} => log!(Error, "main block not defined"),
            BlockParseError::NamingRuleViolation { pos, id } => log!(Warning, "naming rule violation", format!("at:\t{}", pos), format!("id:\t{}", id)),
            BlockParseError::NoStartCommandInMainBlock {} => log!(Error, "no start command in main block"),
            BlockParseError::StartCommandOutsideMainBlock { pos } => log!(Error, "start command outside main block", format!("at:\t{}", pos)),
            BlockParseError::UnknownEscapeSequenceCharacter { pos } => log!(Error, "unknown escape sequence character", format!("at:\t{}", pos)),
        }
    }
}

// note: プリミティブ関数の名前一覧
const PRIM_FUNC_NAMES: &[&'static str] = &["JOIN"];

pub struct BlockParser {
    cons: Rc<RefCell<Console>>,
    start_rule_id: Option<String>,
    file_alias_name: String,
    appeared_block_ids: Box<HashMap<String, CharacterPosition>>,
    block_name: String,
    block_alias_map: HashMap<String, String>,
    file_path: String,
    file_content: Box<String>,
}

impl BlockParser {
    // note: FileMap から最終的な RuleMap を取得する
    pub fn get_rule_map(cons: Rc<RefCell<Console>>, fcpeg_file_map: &mut FCPEGFileMap, enable_memoization: bool) -> ConsoleResult<Box<RuleMap>> {
        let block_map = FCPEGBlock::get_block_map();
        let rule_map = Box::new(RuleMap::new(vec![block_map], ".Syntax.FCPEG".to_string())?);

        let mut parser = SyntaxParser::new(cons.clone(), rule_map, enable_memoization)?;
        // note: HashMap<エイリアス名, ブロックマップ>
        let mut block_maps = Vec::<BlockMap>::new();
        let mut appeared_block_ids = Box::new(HashMap::<String, CharacterPosition>::new());
        let mut start_rule_id = Option::<String>::None;

        for (file_alias_name, fcpeg_file) in fcpeg_file_map.iter() {
            let mut block_parser = BlockParser {
                cons: cons.clone(),
                start_rule_id: None,
                file_alias_name: file_alias_name.clone(),
                appeared_block_ids: appeared_block_ids,
                block_name: String::new(),
                block_alias_map: HashMap::new(),
                file_path: fcpeg_file.file_path.clone(),
                file_content: fcpeg_file.file_content.clone(),
            };

            let tree = Box::new(block_parser.to_syntax_tree(&mut parser)?);
            block_maps.push(block_parser.to_block_map(tree)?);

            if block_parser.file_alias_name == "" {
                start_rule_id = block_parser.start_rule_id.clone();
            }

            appeared_block_ids = block_parser.appeared_block_ids;
        }

        let rule_map = match start_rule_id {
            Some(id) => Box::new(RuleMap::new(block_maps, id)?),
            None => {
                cons.borrow_mut().append_log(BlockParseError::NoStartCommandInMainBlock {}.get_log());
                return Err(());
            },
        };

        let mut has_id_error = false;

        for (each_rule_id, each_pos) in *appeared_block_ids {
            if !rule_map.rule_map.contains_key(&each_rule_id) {
                cons.borrow_mut().append_log(SyntaxParseError::UnknownRuleID {
                    pos: each_pos, rule_id: each_rule_id,
                }.get_log());

                has_id_error = true;
            }
        }

        return if has_id_error {
            Err(())
        } else {
            Ok(rule_map)
        };
    }

    fn to_syntax_tree(&mut self, parser: &mut SyntaxParser) -> ConsoleResult<SyntaxTree> {
        let tree = parser.parse(self.file_path.clone(), &self.file_content)?;

        if cfg!(debug) {
            tree.print(true);
        }

        return Ok(tree);
    }

    // note: FCPEG コードの構文木 → ブロックマップの変換
    fn to_block_map(&mut self, tree: Box<SyntaxTree>) -> ConsoleResult<BlockMap> {
        let mut block_map = BlockMap::new();
        let root = tree.get_child_ref();
        let block_nodes = match root.get_node(&self.cons)?.get_node_child_at(&self.cons, 0) {
            Ok(v) => v.get_reflectable_children(),
            Err(()) => return Ok(block_map),
        };

        for each_block_elem in &block_nodes {
            let each_block_node = each_block_elem.get_node(&self.cons)?;
            let block_name_node = each_block_node.get_node_child_at(&self.cons, 0)?;
            let block_pos = block_name_node.get_position(&self.cons)?;
            self.block_name = block_name_node.join_child_leaf_values();

            if !BlockParser::is_pascal_case(&self.block_name) {
                self.cons.borrow_mut().append_log(BlockParseError::NamingRuleViolation {
                    pos: block_pos.clone(),
                    id: self.block_name.clone(),
                }.get_log());
            }

            if block_map.contains_key(&self.block_name) {
                self.cons.borrow_mut().append_log(BlockParseError::DuplicatedBlockName {
                    pos: block_name_node.get_position(&self.cons)?,
                    block_name: self.block_name.clone(),
                }.get_log());

                return Err(());
            }

            let mut cmds = Vec::<BlockCommand>::new();
            let mut rule_names = Vec::<String>::new();

            match each_block_node.get_node_child_at(&self.cons, 1) {
                Ok(cmd_elems) => {
                    for each_cmd_elem in &cmd_elems.get_reflectable_children() {
                        let each_cmd_node = each_cmd_elem.get_node(&self.cons)?.get_node_child_at(&self.cons, 0)?;
                        let new_cmd = self.to_block_cmd(each_cmd_node)?;

                        // ルール名の重複チェック
                        match &new_cmd {
                            BlockCommand::Define { pos: _, rule } => {
                                if rule_names.contains(&rule.name) {
                                    self.cons.borrow_mut().append_log(BlockParseError::DuplicatedRuleName {
                                        pos: rule.pos.clone(),
                                        rule_name: rule.name.clone(),
                                    }.get_log());

                                    return Err(());
                                }

                                rule_names.push(rule.name.clone())
                            },
                            _ => (),
                        }

                        cmds.push(new_cmd);
                    }
                },
                Err(()) => self.cons.borrow_mut().pop_log(),
            }

            block_map.insert(self.block_name.clone(), Box::new(Block::new(self.block_name.clone(), cmds)));
            self.block_alias_map.clear();
        }

        block_map.insert(String::new(), Box::new(Block::new("Main".to_string(), vec![])));

        if cfg!(debug) {
            for (_, each_block) in &block_map {
                each_block.print();
                println!();
            }
        }

        return Ok(block_map);
    }

    fn to_block_cmd(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<BlockCommand> {
        return match &cmd_node.ast_reflection_style {
            ASTReflectionStyle::Reflection(node_name) => match node_name.as_str() {
                ".Block.CommentCmd" => self.to_comment_cmd(cmd_node),
                ".Block.DefineCmd" => self.to_define_cmd(cmd_node),
                ".Block.StartCmd" => {
                    let start_cmd = self.to_start_cmd(cmd_node)?;

                    match start_cmd.clone() {
                        BlockCommand::Start { pos, file_alias_name, block_name, rule_name } => {
                            if self.block_name != "Main" {
                                self.cons.borrow_mut().append_log(BlockParseError::StartCommandOutsideMainBlock {
                                    pos: pos,
                                }.get_log());

                                return Err(());
                            }

                            if self.file_alias_name == "" {
                                if self.start_rule_id.is_some() {
                                    self.cons.borrow_mut().append_log(BlockParseError::DuplicatedStartCommand {
                                        pos: pos,
                                    }.get_log());

                                    return Err(());
                                }

                                self.start_rule_id = Some(format!("{}.{}.{}", file_alias_name, block_name, rule_name));
                            }
                        }
                        _ => (),
                    }

                    Ok(start_cmd)
                },
                ".Block.UseCmd" => {
                    let use_cmd = self.to_use_cmd(cmd_node)?;

                    match &use_cmd {
                        BlockCommand::Use { pos: _, file_alias_name, block_name, block_alias_name } => {
                            self.block_alias_map.insert(block_alias_name.clone(), format!("{}.{}", file_alias_name, block_name));
                        },
                        _ => (),
                    }

                    Ok(use_cmd)
                },
                _ => {
                    self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                        cause: format!("invalid node name '{}'", node_name),
                    }.get_log());

                    return Err(());
                },
            },
            _ => {
                self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                    cause: "invalid operation".to_string(),
                }.get_log());

                return Err(());
            },
        };
    }

    fn to_comment_cmd(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<BlockCommand> {
        return Ok(BlockCommand::Comment { pos: CharacterPosition::get_empty(), value: cmd_node.join_child_leaf_values() });
    }

    fn to_define_cmd(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<BlockCommand> {
        let rule_name_node = cmd_node.get_node_child_at(&self.cons, 0)?;
        let rule_pos = rule_name_node.get_position(&self.cons)?;
        let rule_name = rule_name_node.join_child_leaf_values();

        if !BlockParser::is_pascal_case(&rule_name) {
            self.cons.borrow_mut().append_log(BlockParseError::NamingRuleViolation {
                pos: rule_pos.clone(),
                id: rule_name.clone(),
            }.get_log());
        }

        let generics_args = match cmd_node.find_first_child_node(vec![".Block.DefineCmdGenericsIDs"]) {
            Some(generics_ids_node) => self.to_define_cmd_arg_ids(generics_ids_node)?,
            None => vec![],
        };

        let func_args = match cmd_node.find_first_child_node(vec![".Block.DefineCmdFuncIDs"]) {
            Some(generics_ids_node) => self.to_define_cmd_arg_ids(generics_ids_node)?,
            None => vec![],
        };

        let new_choice = match cmd_node.find_first_child_node(vec![".Rule.PureChoice"]) {
            Some(choice_node) => Box::new(self.to_rule_choice_elem(choice_node, &generics_args)?),
            None => {
                self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                    msg: "pure choice not found".to_string(),
                }.get_log());

                return Err(());
            },
        };

        let rule = Rule::new(rule_pos, format!("{}.{}.{}", self.file_alias_name, self.block_name, rule_name), rule_name, generics_args, func_args, new_choice);
        return Ok(BlockCommand::Define { pos: CharacterPosition::get_empty(), rule: rule });
    }

    fn to_define_cmd_arg_ids(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<Vec<String>> {
        let mut args = Vec::<String>::new();

        for each_elem in &cmd_node.sub_elems {
            match each_elem {
                SyntaxNodeElement::Node(each_node) => {
                    if each_node.ast_reflection_style == ASTReflectionStyle::Reflection(".Rule.ArgID".to_string()) {
                        let new_arg = each_node.join_child_leaf_values();

                        if args.contains(&new_arg) {
                            self.cons.borrow_mut().append_log(BlockParseError::DuplicatedArgumentID {
                                pos: each_node.get_position(&self.cons)?,
                                arg_id: new_arg.clone(),
                            }.get_log());
                        }

                        args.push(new_arg);
                    }
                },
                _ => (),
            }
        }

        return Ok(args);
    }

    fn to_start_cmd(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<BlockCommand> {
        let raw_id_node = cmd_node.get_node_child_at(&self.cons, 0)?;
        let raw_id = self.to_chain_id(raw_id_node)?;
        let divided_raw_id = raw_id.split(".").collect::<Vec<&str>>();

        let cmd = match divided_raw_id.len() {
            2 => BlockCommand::Start { pos: CharacterPosition::get_empty(), file_alias_name: String::new(), block_name: divided_raw_id.get(0).unwrap().to_string(), rule_name: divided_raw_id.get(1).unwrap().to_string() },
            3 => BlockCommand::Start { pos: CharacterPosition::get_empty(), file_alias_name: divided_raw_id.get(0).unwrap().to_string(), block_name: divided_raw_id.get(1).unwrap().to_string(), rule_name: divided_raw_id.get(2).unwrap().to_string() },
            _ => {
                self.cons.borrow_mut().append_log(BlockParseError::InvalidID {
                    pos: raw_id_node.get_node_child_at(&self.cons, 0)?.get_position(&self.cons)?,
                    id: raw_id,
                }.get_log());

                return Err(());
            },
        };

        return Ok(cmd);
    }

    fn to_use_cmd(&mut self, cmd_node: &SyntaxNode) -> ConsoleResult<BlockCommand> {
        let raw_id = self.to_chain_id(cmd_node.get_node_child_at(&self.cons, 0)?)?;
        let divided_raw_id = raw_id.split(".").collect::<Vec<&str>>();

        let (file_alias_name, block_alias_id) = match cmd_node.find_first_child_node(vec![".Block.UseCmdBlockAlias"]) {
            Some(v) => (divided_raw_id.get(0).unwrap().to_string(), v.get_node_child_at(&self.cons, 0)?.join_child_leaf_values()),
            None => {
                match divided_raw_id.len() {
                    1 => (self.file_alias_name.clone(), divided_raw_id.get(0).unwrap().to_string()),
                    2 => (divided_raw_id.get(0).unwrap().to_string(), divided_raw_id.get(1).unwrap().to_string()),
                    _ => {
                        self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                            msg: "invalid chain ID length on use command".to_string(),
                        }.get_log());

                        return Err(());
                    },
                }
            }
        };

        return match divided_raw_id.len() {
            1 => Ok(BlockCommand::Use { pos: CharacterPosition::get_empty(), file_alias_name: file_alias_name, block_name: divided_raw_id.get(0).unwrap().to_string(), block_alias_name: block_alias_id }),
            2 => Ok(BlockCommand::Use { pos: CharacterPosition::get_empty(), file_alias_name: file_alias_name, block_name: divided_raw_id.get(1).unwrap().to_string(), block_alias_name: block_alias_id }),
            _ => {
                self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                    msg: "invalid chain ID length on use command".to_string(),
                }.get_log());

                return Err(());
            },
        };
    }

    // note: Seq を解析する
    fn to_seq_elem(&mut self, seq_node: &SyntaxNode, generics_args: &Vec<String>) -> ConsoleResult<RuleElement> {
        let mut children = Vec::<RuleElement>::new();

        // note: SeqElem ノードをループ
        for each_seq_elem_elem in &seq_node.get_reflectable_children() {
            let each_seq_elem_node = each_seq_elem_elem.get_node(&self.cons)?;

            // note: Lookahead ノード
            let lookahead_kind = match each_seq_elem_node.find_first_child_node(vec![".Rule.Lookahead"]) {
                Some(v) => {
                    match v.get_leaf_child_at(&self.cons, 0)?.value.as_str() {
                        "&" => RuleElementLookaheadKind::Positive,
                        "!" => RuleElementLookaheadKind::Negative,
                        _ => {
                            self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                                cause: format!("unknown lookahead kind"),
                            }.get_log());

                            return Err(());
                        },
                    }
                },
                None => RuleElementLookaheadKind::None,
            };

            // note: Loop ノード
            let loop_count = match each_seq_elem_node.find_first_child_node(vec![".Rule.Loop"]) {
                Some(v) => {
                    match v.get_child_at(&self.cons, 0)? {
                        SyntaxNodeElement::Node(node) => {
                            let min_num = match node.get_child_at(&self.cons, 0)? {
                                SyntaxNodeElement::Node(min_node) => {
                                    let min_str = min_node.join_child_leaf_values();

                                    match min_str.parse::<usize>() {
                                        Ok(v) => {
                                            if v == 0 {
                                                self.cons.borrow_mut().append_log(BlockParseError::InvalidLoopCount {
                                                    pos: CharacterPosition::get_empty(),
                                                }.get_log());

                                                return Err(());
                                            }

                                            v
                                        },
                                        Err(_) => {
                                            self.cons.borrow_mut().append_log(BlockParseError::InvalidLoopCount {
                                                pos: CharacterPosition::get_empty(),
                                            }.get_log());

                                            return Err(());
                                        },
                                    }
                                },
                                SyntaxNodeElement::Leaf(_) => 0usize,
                            };

                            let max_num = match node.get_child_at(&self.cons, 1)? {
                                SyntaxNodeElement::Node(max_node) => {
                                    let max_str = max_node.join_child_leaf_values();

                                    match max_str.parse::<usize>() {
                                        Ok(v) => Infinitable::Normal(v),
                                        Err(_) => {
                                            self.cons.borrow_mut().append_log(BlockParseError::InvalidLoopCount {
                                                pos: CharacterPosition::get_empty(),
                                            }.get_log());

                                            return Err(());
                                        },
                                    }
                                },
                                SyntaxNodeElement::Leaf(_) => Infinitable::Infinite,
                            };

                            RuleElementLoopCount::new(min_num, max_num)
                        },
                        SyntaxNodeElement::Leaf(leaf) => {
                            match leaf.value.as_str() {
                                "?" | "*" | "+" => RuleElementLoopCount::from_symbol(&leaf.value),
                                _ => {
                                    self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                                        cause: format!("unknown lookahead kind"),
                                    }.get_log());

                                    return Err(());
                                },
                            }
                        }
                    }
                },
                None => RuleElementLoopCount::get_single_loop(),
            };

            // note: ASTReflectionStyle ノード
            // todo: 構成ファイルによって切り替える
            let ast_reflection_style = match each_seq_elem_node.find_first_child_node(vec![".Rule.ASTReflectionStyle"]) {
                Some(style_node) => {
                    match style_node.get_leaf_child_at(&self.cons, 0) {
                        Ok(leaf) => {
                            if leaf.value == "##" {
                                ASTReflectionStyle::Expansion
                            } else {
                                ASTReflectionStyle::Reflection(style_node.join_child_leaf_values())
                            }
                        },
                        Err(()) => {
                            self.cons.borrow_mut().pop_log();
                            ASTReflectionStyle::from_config(false, true, String::new())
                        },
                    }
                },
                None => ASTReflectionStyle::from_config(false, false, String::new()),
            };

            // note: Choice または Expr ノード
            let choice_or_expr_node = match each_seq_elem_node.find_first_child_node(vec![".Rule.Choice", ".Rule.Expr"]) {
                Some(v) => v,
                None => {
                    self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                        cause: "invalid operation".to_string(),
                    }.get_log());

                    return Err(());
                },
            };

            match &choice_or_expr_node.ast_reflection_style {
                ASTReflectionStyle::Reflection(name) => {
                    let new_elem = match name.as_str() {
                        ".Rule.Choice" => {
                            let mut new_choice = Box::new(self.to_rule_choice_elem(choice_or_expr_node.get_node_child_at(&self.cons, 0)?, generics_args)?);
                            new_choice.lookahead_kind = lookahead_kind;
                            new_choice.loop_count = loop_count;
                            new_choice.ast_reflection_style = ast_reflection_style;
                            RuleElement::Group(new_choice)
                        },
                        ".Rule.Expr" => {
                            let mut new_expr = Box::new(self.to_rule_expr_elem(choice_or_expr_node, generics_args)?);
                            new_expr.lookahead_kind = lookahead_kind;
                            new_expr.loop_count = loop_count;
                            new_expr.ast_reflection_style = ast_reflection_style;
                            RuleElement::Expression(new_expr)
                        },
                        _ => {
                            self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                                cause: format!("invalid node name '{}'", name),
                            }.get_log());

                            return Err(());
                        },
                    };

                    children.push(new_elem);
                },
                _ => {
                    self.cons.borrow_mut().append_log(SyntaxParseError::InvalidSyntaxTreeStructure {
                        cause: "invalid operation".to_string()
                    }.get_log());

                    return Err(());
                },
            };
        }

        let mut seq = Box::new(RuleGroup::new(RuleGroupKind::Sequence));
        seq.sub_elems = children;
        return Ok(RuleElement::Group(seq));
    }

    // note: Rule.PureChoice ノードの解析
    fn to_rule_choice_elem(&mut self, choice_node: &SyntaxNode, generics_args: &Vec<String>) -> ConsoleResult<RuleGroup> {
        let mut children = Vec::<RuleElement>::new();
        let mut group_kind = RuleGroupKind::Sequence;

        // Seq ノードをループ
        for seq_elem in &choice_node.get_reflectable_children() {
            match &seq_elem {
                SyntaxNodeElement::Node(node) => {
                    match &seq_elem.get_ast_reflection_style() {
                        ASTReflectionStyle::Reflection(name) => if name == ".Rule.Seq" {
                            let new_child = self.to_seq_elem(node, generics_args)?;
                            children.push(new_child);
                        },
                        _ => (),
                    }
                },
                SyntaxNodeElement::Leaf(leaf) => {
                    match leaf.value.as_str() {
                        ":" | "," => group_kind = RuleGroupKind::Choice,
                        _ => (),
                    }
                },
            }
        }

        let mut group = Box::new(RuleGroup::new(group_kind.clone()));
        group.sub_elems = children;

        let mut tmp_root_group = RuleGroup::new(RuleGroupKind::Sequence);
        tmp_root_group.sub_elems = vec![RuleElement::Group(group)];

        return Ok(tmp_root_group);
    }

    fn to_rule_expr_elem(&mut self, expr_node: &SyntaxNode, generics_args: &Vec<String>) -> ConsoleResult<RuleExpression> {
        let expr_child_node = expr_node.get_node_child_at(&self.cons, 0)?;
        let (pos, kind, value) = match &expr_child_node.ast_reflection_style {
            ASTReflectionStyle::Reflection(name) => {
                match name.as_str() {
                    ".Rule.ArgID" => (expr_child_node.get_position(&self.cons)?, RuleExpressionKind::ArgID, expr_child_node.join_child_leaf_values()),
                    ".Rule.CharClass" => (expr_child_node.get_position(&self.cons)?, RuleExpressionKind::CharClass, format!("[{}]", expr_child_node.join_child_leaf_values())),
                    ".Rule.Generics" | ".Rule.Func" => {
                        let mut args = Vec::<Box<RuleGroup>>::new();
                        for instant_pure_choice_node in expr_child_node.find_child_nodes(vec![".Rule.InstantPureChoice"]) {
                            args.push(Box::new(self.to_rule_choice_elem(instant_pure_choice_node, generics_args)?));
                        }

                        let parent_node = expr_child_node.get_node_child_at(&self.cons, 0)?.get_node_child_at(&self.cons, 0)?;
                        let pos = parent_node.get_position(&self.cons)?;

                        let generics = match name.as_str() {
                            ".Rule.Generics" => RuleExpressionKind::Generics(args),
                            ".Rule.Func" => RuleExpressionKind::Func(args),
                            _ => {
                                self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                                    msg: "invalid operation".to_string(),
                                }.get_log());

                                return Err(());
                            },
                        };

                        let raw_id = BlockParser::to_string_vec(&self.cons, expr_child_node.get_node_child_at(&self.cons, 0)?)?;
                        let joined_raw_id = raw_id.join(".");
                        let id = if name == ".Rule.Func" && PRIM_FUNC_NAMES.contains(&joined_raw_id.as_str()) {
                            joined_raw_id.clone()
                        } else {
                            match BlockParser::to_rule_id(&self.cons, &pos, &raw_id, &self.block_alias_map, &self.file_alias_name, &self.block_name) {
                                Ok(id) => {
                                    if !self.appeared_block_ids.contains_key(&id) {
                                        self.appeared_block_ids.insert(id.clone(), pos.clone());
                                    }

                                    id
                                },
                                Err(()) => return Err(()),
                            }
                        };

                        (pos, generics, id)
                    },
                    ".Rule.ID" => {
                        let chain_id_node = expr_child_node.get_node_child_at(&self.cons, 0)?;
                        let parent_node = chain_id_node.get_node_child_at(&self.cons, 0)?;
                        let pos = parent_node.get_position(&self.cons)?;

                        let id = match BlockParser::to_rule_id(&self.cons, &pos, &BlockParser::to_string_vec(&self.cons, chain_id_node)?, &self.block_alias_map, &self.file_alias_name, &self.block_name) {
                            Ok(id) => {
                                if !self.appeared_block_ids.contains_key(&id) {
                                    self.appeared_block_ids.insert(id.clone(), pos.clone());
                                }

                                id
                            },
                            Err(()) => return Err(()),
                        };

                        (pos, RuleExpressionKind::ID, id)
                    },
                    ".Rule.Str" => (CharacterPosition::get_empty(), RuleExpressionKind::String, self.to_string_value(expr_child_node)?),
                    ".Rule.Wildcard" => (expr_child_node.get_position(&self.cons)?, RuleExpressionKind::Wildcard, ".".to_string()),
                    _ => {
                        self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                            msg: format!("unknown expression name '{}'", name),
                        }.get_log());

                        return Err(());
                    },
                }
            },
            _ => {
                self.cons.borrow_mut().append_log(SyntaxParseError::InternalError {
                    msg: "invalid operation".to_string(),
                }.get_log());

                return Err(());
            },
        };

        let expr = RuleExpression::new(pos, kind, value);
        return Ok(expr);
    }

    fn to_string_vec(cons: &Rc<RefCell<Console>>, str_vec_node: &SyntaxNode) -> ConsoleResult<Vec<String>> {
        let mut str_vec = Vec::<String>::new();

        for str_elem in str_vec_node.get_reflectable_children() {
            str_vec.push(str_elem.get_node(cons)?.join_child_leaf_values());
        }

        return Ok(str_vec);
    }

    fn to_rule_id(cons: &Rc<RefCell<Console>>, pos: &CharacterPosition, id_tokens: &Vec<String>, block_alias_map: &HashMap<String, String>, file_alias_name: &String, block_name: &String) -> ConsoleResult<String> {
        let (new_id, _id_file_alias_name, id_block_name, id_rule_name) = match id_tokens.len() {
            1 => {
                let id_rule_name = id_tokens.get(0).unwrap();
                let new_id = format!("{}.{}.{}", file_alias_name, block_name, id_rule_name);

                (new_id, file_alias_name.as_str(), block_name, id_rule_name.clone())
            },
            2 => {
                let block_name = id_tokens.get(0).unwrap().to_string();
                let rule_name = id_tokens.get(1).unwrap().to_string();

                if block_alias_map.contains_key(&block_name.to_string()) {
                    let block_name = block_alias_map.get(&block_name.to_string()).unwrap();
                    // note: ブロック名がエイリアスである場合
                    let new_id = format!("{}.{}", block_name, rule_name);

                    (new_id, "", block_name, rule_name.clone())
                } else {
                    // note: ブロック名がエイリアスでない場合
                    cons.borrow_mut().append_log(BlockParseError::BlockAliasNotFound {
                        pos: pos.clone(),
                        block_alias_name: block_name.to_string(),
                    }.get_log());

                    return Err(());
                }
            },
            3 => {
                let file_alias_name = id_tokens.get(0).unwrap();
                let block_name = id_tokens.get(1).unwrap();
                let rule_name = id_tokens.get(2).unwrap();
                let new_id = format!("{}.{}.{}", file_alias_name, block_name, rule_name);

                (new_id, file_alias_name.as_str(), block_name, rule_name.to_string())
            },
            _ => {
                cons.borrow_mut().append_log(BlockParseError::InternalError {
                    msg: format!("invalid id expression"),
                }.get_log());

                return Err(());
            },
        };

        // note: プライベート規則の外部アクセスを除外
        // todo: プライベートブロックに対応
        // todo: 異なるファイルでの同ブロック名を除外
        if id_rule_name.starts_with("_") && *block_name != *id_block_name {
            cons.borrow_mut().append_log(BlockParseError::AttemptToAccessPrivateItem {
                pos: pos.clone(),
                item_id: new_id.clone(),
            }.get_log());
        }

        return Ok(new_id);
    }

    fn to_string_value(&mut self, str_node: &SyntaxNode) -> ConsoleResult<String> {
        let mut s = String::new();

        for each_elem in &str_node.sub_elems {
            match each_elem {
                SyntaxNodeElement::Node(node) => {
                    match node.ast_reflection_style {
                        ASTReflectionStyle::Reflection(_) => {
                            s += match node.get_leaf_child_at(&self.cons, 0)?.value.as_str() {
                                "\\" => "\\",
                                "\"" => "\"",
                                "n" => "\n",
                                "t" => "\t",
                                "z" => "\0",
                                _ => {
                                    self.cons.borrow_mut().append_log(BlockParseError::UnknownEscapeSequenceCharacter {
                                        pos: node.get_position(&self.cons)?,
                                    }.get_log());

                                    return Err(());
                                },
                            };
                        },
                        _ => (),
                    }
                },
                SyntaxNodeElement::Leaf(leaf) => {
                    match leaf.ast_reflection_style {
                        ASTReflectionStyle::Reflection(_) => s += leaf.value.as_ref(),
                        _ => (),
                    }
                },
            }
        }

        return Ok(s);
    }

    fn to_chain_id(&mut self, chain_id_node: &SyntaxNode) -> ConsoleResult<String> {
        let mut ids = Vec::<String>::new();

        for chain_id_elem in &chain_id_node.get_reflectable_children() {
            ids.push(chain_id_elem.get_node(&self.cons)?.join_child_leaf_values());
        }

        return Ok(ids.join("."));
    }

    // ret: 空文字の場合は false
    fn is_pascal_case(id: &String) -> bool {
        let mut id_chars = id.chars();

        return match id_chars.next() {
            Some(first_char) => {
                if first_char != '_' {
                    first_char.is_uppercase()
                } else {
                    match id_chars.next() {
                        Some(second_char) => second_char.is_uppercase(),
                        None => false,
                    }
                }
            },
            None => false,
        };
    }
}

struct FCPEGBlock {}

impl FCPEGBlock {
    pub fn get_block_map() -> BlockMap {
        return block_map!{
            "Main" => get_main_block,
            "Syntax" => get_syntax_block,
            "Symbol" => get_symbol_block,
            "Misc" => get_misc_block,
            "Block" => get_block_block,
            "Rule" => get_rule_block,
        };
    }

    fn get_main_block() -> Block {
        let start_cmd = start_cmd!("", "Syntax", "FCPEG");
        return block!("Main", vec![start_cmd]);
    }

    fn get_syntax_block() -> Block {
        // code: FCPEG <- Symbol.Space*# Symbol.LineEnd*# (Block.Block Symbol.LineEnd+#)* Symbol.LineEnd*# Symbol.Space*# Symbol.EOF#,
        let fcpeg_rule = rule!{
            ".Syntax.FCPEG",
            choice!{
                vec![],
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(ID, ".Symbol.LineEnd", "*", "#"),
                choice!{
                    vec!["*"],
                    choice!{
                        vec![],
                        expr!(ID, ".Block.Block"),
                        expr!(ID, ".Symbol.LineEnd", "+", "#"),
                    },
                },
                expr!(ID, ".Symbol.LineEnd", "*", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "\0", "#"),
            },
        };

        return block!(".Syntax", vec![fcpeg_rule]);
    }

    fn get_symbol_block() -> Block {
        // code: Space <- " ",
        let space_rule = rule!{
            ".Symbol.Space",
            choice!{
                vec![],
                expr!(String, " "),
            },
        };

        // code: LineEnd <- Space* "\n" Space*,
        let line_end_rule = rule!{
            ".Symbol.LineEnd",
            choice!{
                vec![],
                expr!(ID, ".Symbol.Space", "*"),
                expr!(String, "\n"),
                expr!(ID, ".Symbol.Space", "*"),
            },
        };

        // code: EOF <- "\z",
        let eof_rule = rule!{
            ".Symbol.EOF",
            choice!{
                vec![],
                expr!(String, "\0", "#"),
            },
        };

        return block!(".Symbol", vec![space_rule, line_end_rule, eof_rule]);
    }

    fn get_misc_block() -> Block {
        // code: SingleID <- [a-zA-Z_] [a-zA-Z0-9_]*,
        let single_id_rule = rule!{
            ".Misc.SingleID",
            choice!{
                vec![],
                expr!(CharClass, "[a-zA-Z_]"),
                expr!(CharClass, "[a-zA-Z0-9_]", "*"),
            },
        };

        // code: ChainID <- SingleID ("."# SingleID)*##,
        let chain_id_rule = rule!{
            ".Misc.ChainID",
            choice!{
                vec![],
                expr!(ID, ".Misc.SingleID"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec![],
                        expr!(String, ".", "#"),
                        expr!(ID, ".Misc.SingleID"),
                    },
                },
            },
        };

        return block!(".Misc", vec![single_id_rule, chain_id_rule]);
    }

    fn get_block_block() -> Block {
        // code: Block <- "["# Symbol.Space*# Misc.SingleID Symbol.Space*# "]"# Symbol.Space*# "{"# Symbol.LineEnd+# (Cmd Symbol.LineEnd+#)* "}"#,
        let block_rule = rule!{
            ".Block.Block",
            choice!{
                vec![],
                expr!(String, "[", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(ID, ".Misc.SingleID"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "]", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "{", "#"),
                expr!(ID, ".Symbol.LineEnd", "+", "#"),
                choice!{
                    vec!["*"],
                    choice!{
                        vec![],
                        expr!(ID, ".Block.Cmd"),
                        expr!(ID, ".Symbol.LineEnd", "+", "#"),
                    },
                },
                expr!(String, "}", "#"),
            },
        };

        // code: Cmd <- CommentCmd : DefineCmd : StartCmd : UseCmd,
        let cmd_rule = rule!{
            ".Block.Cmd",
            choice!{
                vec![":"],
                choice!{
                    vec![],
                    expr!(ID, ".Block.CommentCmd"),
                },
                choice!{
                    vec![],
                    expr!(ID, ".Block.DefineCmd"),
                },
                choice!{
                    vec![],
                    expr!(ID, ".Block.StartCmd"),
                },
                choice!{
                    vec![],
                    expr!(ID, ".Block.UseCmd"),
                },
            },
        };

        // code: CommentCmd <- "%"# (!"," !Symbol.LineEnd .)*## ","#,
        let comment_rule = rule!{
            ".Block.CommentCmd",
            choice!{
                vec![],
                expr!(String, "%", "#"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec![],
                        expr!(String, ",", "!"),
                        expr!(ID, ".Symbol.LineEnd", "!"),
                        expr!(Wildcard, "."),
                    },
                },
                expr!(String, ",", "#"),
            },
        };

        // code: DefineCmd <- Misc.SingleID DefineCmdGenericsIDs? DefineCmdFuncIDs? Symbol.Space*# "<-"# Symbol.Space*# Rule.PureChoice Symbol.Space*# ","#,
        let define_cmd_rule = rule!{
            ".Block.DefineCmd",
            choice!{
                vec![],
                expr!(ID, ".Misc.SingleID"),
                expr!(ID, ".Block.DefineCmdGenericsIDs", "?"),
                expr!(ID, ".Block.DefineCmdFuncIDs", "?"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "<-", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(ID, ".Rule.PureChoice"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, ",", "#"),
            },
        };

        // code: DefineCmdGenericsIDs <- "<"# Rule.ArgID (","# Symbol.Space# Rule.ArgID)*## ">"#,
        let define_cmd_generics_ids_rule = rule!{
            ".Block.DefineCmdGenericsIDs",
            choice!{
                vec![],
                expr!(String, "<", "#"),
                expr!(ID, ".Rule.ArgID"),
                choice!{
                    vec!["*", "##"],
                    expr!(String, ",", "#"),
                    expr!(ID, ".Symbol.Space", "#"),
                    expr!(ID, ".Rule.ArgID"),
                },
                expr!(String, ">", "#"),
            },
        };

        // code: DefineCmdFuncIDs <- "("# Rule.ArgID (","# Symbol.Space# Rule.ArgID)*## ")"#,
        let define_cmd_func_ids_rule = rule!{
            ".Block.DefineCmdFuncIDs",
            choice!{
                vec![],
                expr!(String, "(", "#"),
                expr!(ID, ".Rule.ArgID"),
                choice!{
                    vec!["*", "##"],
                    expr!(String, ",", "#"),
                    expr!(ID, ".Symbol.Space", "#"),
                    expr!(ID, ".Rule.ArgID"),
                },
                expr!(String, ")", "#"),
            },
        };

        // code: StartCmd <- "+"# Symbol.Space*# "start"# Symbol.Space+# Misc.ChainID Symbol.Space*# ","#,
        let start_cmd_rule = rule!{
            ".Block.StartCmd",
            choice!{
                vec![],
                expr!(String, "+", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "start", "#"),
                expr!(ID, ".Symbol.Space", "+", "#"),
                expr!(ID, ".Misc.ChainID"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, ",", "#"),
            },
        };

        // code: UseCmd <- "+"# Symbol.Space*# "use"# Symbol.Space+# Misc.ChainID UseCmdBlockAlias? Symbol.Space*# ","#,
        let use_cmd_rule = rule!{
            ".Block.UseCmd",
            choice!{
                vec![],
                expr!(String, "+", "#"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, "use", "#"),
                expr!(ID, ".Symbol.Space", "+", "#"),
                expr!(ID, ".Misc.ChainID"),
                expr!(ID, ".Block.UseCmdBlockAlias", "?"),
                expr!(ID, ".Symbol.Space", "*", "#"),
                expr!(String, ",", "#"),
            },
        };

        // code: UseCmdBlockAlias <- Symbol.Space+# "as" Symbol.Space+# Misc.SingleID,
        let use_cmd_block_alias_rule = rule!{
            ".Block.UseCmdBlockAlias",
            choice!{
                vec![],
                expr!(ID, ".Symbol.Space", "+", "#"),
                expr!(String, "as", "#"),
                expr!(ID, ".Symbol.Space", "+", "#"),
                expr!(ID, ".Misc.SingleID"),
            },
        };

        return block!(".Block", vec![block_rule, cmd_rule, comment_rule, define_cmd_rule, define_cmd_generics_ids_rule, define_cmd_func_ids_rule, start_cmd_rule, use_cmd_rule, use_cmd_block_alias_rule]);
    }

    fn get_rule_block() -> Block {
        // code: InstantPureChoice <- Seq ":" Symbol.Space# Seq)*##,
        let instant_pure_choice_rule = rule!{
            ".Rule.InstantPureChoice",
            choice!{
                vec![],
                expr!(ID, ".Rule.Seq"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec!["##"],
                        expr!(String, ":"),
                        expr!(ID, ".Symbol.Space", "#"),
                        expr!(ID, ".Rule.Seq"),
                    },
                },
            },
        };

        // code: PureChoice <- Seq ((SeqDiv+# ":" SeqDiv+# : ",")## Seq)*##,
        let pure_choice_rule = rule!{
            ".Rule.PureChoice",
            choice!{
                vec![],
                expr!(ID, ".Rule.Seq"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec!["##"],
                        choice!{
                            vec![":"],
                            choice!{
                                vec!["##"],
                                expr!(ID, ".Rule.SeqDiv", "+", "#"),
                                expr!(String, ":"),
                                expr!(ID, ".Rule.SeqDiv", "+", "#"),
                            },
                            choice!{
                                vec!["##"],
                                expr!(String, ","),
                                expr!(ID, ".Symbol.Space", "#"),
                            },
                        },
                        expr!(ID, ".Rule.Seq"),
                    },
                },
            },
        };

        // code: Choice <- "("# PureChoice ")"#,
        let choice_rule = rule!{
            ".Rule.Choice",
            choice!{
                vec![],
                expr!(String, "(", "#"),
                expr!(ID, ".Rule.PureChoice"),
                expr!(String, ")", "#"),
            },
        };

        // code: SeqDiv <- Symbol.Space# : "\n"#,
        let seq_div_rule = rule!{
            ".Rule.SeqDiv",
            choice!{
                vec![],
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(ID, ".Symbol.Space", "#"),
                    },
                    choice!{
                        vec![],
                        expr!(String, "\n", "#"),
                    },
                },
            },
        };

        // code: Seq <- SeqElem (SeqDiv+# SeqElem)*##,
        let seq_rule = rule!{
            ".Rule.Seq",
            choice!{
                vec![],
                expr!(ID, ".Rule.SeqElem"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec![],
                        choice!{
                            vec![],
                            expr!(ID, ".Rule.SeqDiv", "+", "#"),
                            expr!(ID, ".Rule.SeqElem"),
                        },
                    },
                },
            },
        };

        // code: SeqElem <- Lookahead? (Choice : Expr) Loop? RandomOrder? ASTReflectionStyle?,
        let seq_elem_rule = rule!{
            ".Rule.SeqElem",
            choice!{
                vec![],
                expr!(ID, ".Rule.Lookahead", "?"),
                choice!{
                    vec!["##"],
                    choice!{
                        vec![":"],
                        choice!{
                            vec![],
                            expr!(ID, ".Rule.Choice"),
                        },
                        choice!{
                            vec![],
                            expr!(ID, ".Rule.Expr"),
                        },
                    },
                },
                expr!(ID, ".Rule.Loop", "?"),
                expr!(ID, ".Rule.RandomOrder", "?"),
                expr!(ID, ".Rule.ASTReflectionStyle", "?"),
            },
        };

        // code: Expr <- Generics : ArgID : Func : ID : Str : CharClass : Wildcard,
        let expr_rule = rule!{
            ".Rule.Expr",
            choice!{
                vec![],
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.ArgID"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Generics"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Func"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.ID"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Str"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.CharClass"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Wildcard"),
                    },
                },
            },
        };

        // code: Lookahead <- "!" : "&",
        let lookahead_rule = rule!{
            ".Rule.Lookahead",
            choice!{
                vec![],
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(String, "!"),
                    },
                    choice!{
                        vec![],
                        expr!(String, "&"),
                    },
                },
            },
        };

        // code: Loop <- "?" : "*" : "+" : LoopRange,
        let loop_rule = rule!{
            ".Rule.Loop",
            choice!{
                vec![],
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(String, "?"),
                    },
                    choice!{
                        vec![],
                        expr!(String, "*"),
                    },
                    choice!{
                        vec![],
                        expr!(String, "+"),
                    },
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.LoopRange"),
                    },
                },
            },
        };

        // code: LoopRange <- "{"# (Num : "")## ","# (Num : "")## "}"#,
        let loop_range_rule = rule!{
            ".Rule.LoopRange",
            choice!{
                vec![],
                expr!(String, "{", "#"),
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Num", "##"),
                    },
                    choice!{
                        vec!["##"],
                        expr!(String, ""),
                    },
                },
                expr!(String, ",", "#"),
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(ID, ".Rule.Num", "##"),
                    },
                    choice!{
                        vec!["##"],
                        expr!(String, ""),
                    },
                },
                expr!(String, "}", "#"),
            },
        };

        // expr: RandomOrder <- "^"# RandomOrderRange?,
        let random_order_rule = rule!{
            ".Rule.RandomOrder",
            choice!{
                vec![],
                expr!(String, "^", "#"),
                expr!(String, "RandomOrderRange", "?"),
            },
        };

        // code: RandomOrderRange <- "["# Num? ","# Num? "]"#,
        let random_order_range_rule = rule!{
            ".Rule.RandomOrderRange",
            choice!{
                vec![],
                expr!(String, "[", "#"),
                expr!(ID, ".Rule.Num", "?"),
                expr!(String, "ID", "#"),
            },
        };

        // code: ASTReflectionStyle <- "##" : "#"# Misc.SingleID?##,
        let ast_reflection_rule = rule!{
            ".Rule.ASTReflectionStyle",
            choice!{
                vec![],
                choice!{
                    vec![":"],
                    choice!{
                        vec![],
                        expr!(String, "##"),
                    },
                    choice!{
                        vec![],
                        expr!(String, "#", "#"),
                        expr!(ID, ".Misc.SingleID", "?", "##"),
                    },
                },
            },
        };

        // code: Num <- [0-9]+,
        let num_rule = rule!{
            ".Rule.Num",
            choice!{
                vec![],
                expr!(CharClass, "[0-9]+", "+"),
            },
        };

        // code: ID <- Misc.ChainID,
        let id_rule = rule!{
            ".Rule.ID",
            choice!{
                vec![],
                expr!(ID, ".Misc.ChainID"),
            },
        };

        // code: ArgID <- "$"# Misc.SingleID##,
        let arg_id_rule = rule!{
            ".Rule.ArgID",
            choice!{
                vec![],
                expr!(String, "$", "#"),
                expr!(ID, ".Misc.SingleID", "##"),
            },
        };

        // code: Generics <- Misc.ChainID "<"# InstantPureChoice (","# Symbol.Space# InstantPureChoice)*## ">"#,
        let generics_rule = rule!{
            ".Rule.Generics",
            choice!{
                vec![],
                expr!(ID, ".Misc.ChainID"),
                expr!(String, "<", "#"),
                expr!(ID, ".Rule.InstantPureChoice"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec!["##"],
                        expr!(String, ",", "#"),
                        expr!(ID, ".Symbol.Space", "#"),
                        expr!(ID, ".Rule.InstantPureChoice"),
                    },
                },
                expr!(String, ">", "#"),
            },
        };

        // code: Func <- Misc.ChainID "("# InstantPureChoice (","# Symbol.Space# InstantPureChoice)*## ")"#,
        let func_rule = rule!{
            ".Rule.Func",
            choice!{
                vec![],
                expr!(ID, ".Misc.ChainID"),
                expr!(String, "(", "#"),
                expr!(ID, ".Rule.InstantPureChoice"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec!["##"],
                        expr!(String, ",", "#"),
                        expr!(ID, ".Symbol.Space", "#"),
                        expr!(ID, ".Rule.InstantPureChoice"),
                    },
                },
                expr!(String, ")", "#"),
            },
        };

        // code: EscSeq <- "\\"# ("\\" : "\"" : "n" : "t" : "z")##,
        let esc_seq_rule = rule!{
            ".Rule.EscSeq",
            choice!{
                vec![],
                expr!(String, "\\", "#"),
                choice!{
                    vec!["##"],
                    choice!{
                        vec![":"],
                        choice!{
                            vec![],
                            expr!(String, "\\"),
                        },
                        choice!{
                            vec![],
                            expr!(String, "\""),
                        },
                        choice!{
                            vec![],
                            expr!(String, "n"),
                        },
                        choice!{
                            vec![],
                            expr!(String, "t"),
                        },
                        choice!{
                            vec![],
                            expr!(String, "z"),
                        },
                    },
                },
            },
        };

        // code: Str <- "\""# ((EscSeq : !(("\\" : "\"")) .))*## "\""#,
        let str_rule = rule!{
            ".Rule.Str",
            choice!{
                vec![],
                expr!(String, "\"", "#"),
                choice!{
                    vec!["*", "##"],
                    choice!{
                        vec![":"],
                        choice!{
                            vec![],
                            expr!(ID, ".Rule.EscSeq"),
                        },
                        choice!{
                            vec![],
                            choice!{
                                vec!["!"],
                                choice!{
                                    vec![":"],
                                    choice!{
                                        vec![],
                                        expr!(String, "\\"),
                                    },
                                    choice!{
                                        vec![],
                                        expr!(String, "\""),
                                    },
                                },
                            },
                            expr!(Wildcard, "."),
                        },
                    },
                },
                expr!(String, "\"", "#"),
            },
        };

        // code: CharClass <- "["# (!"[" !"]" !Symbol.LineEnd (("\\[" : "\\]" : "\\\\" : .))##)+## "]"#,
        let char_class_rule = rule!{
            ".Rule.CharClass",
            choice!{
                vec![],
                expr!(String, "[", "#"),
                choice!{
                    vec!["+", "##"],
                    expr!(String, "[", "!"),
                    expr!(String, "]", "!"),
                    expr!(ID, ".Symbol.LineEnd", "!"),
                    choice!{
                        vec!["##"],
                        choice!{
                            vec![":"],
                            choice!{
                                vec![],
                                expr!(String, "\\["),
                            },
                            choice!{
                                vec![],
                                expr!(String, "\\]"),
                            },
                            choice!{
                                vec![],
                                expr!(String, "\\\\"),
                            },
                            choice!{
                                vec![],
                                expr!(Wildcard, "."),
                            },
                        },
                    },
                },
                expr!(String, "]", "#"),
            },
        };

        // code: Wildcard <- ".",
        let wildcard_rule = rule!{
            ".Rule.Wildcard",
            choice!{
                vec![],
                expr!(String, "."),
            },
        };

        return block!(".Rule", vec![instant_pure_choice_rule, pure_choice_rule, choice_rule, seq_div_rule, seq_rule, seq_elem_rule, expr_rule, lookahead_rule, loop_rule, loop_range_rule, random_order_rule, random_order_range_rule, ast_reflection_rule, num_rule, id_rule, arg_id_rule, generics_rule, func_rule, esc_seq_rule, str_rule, char_class_rule, wildcard_rule]);
    }
}
