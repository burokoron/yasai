use crate::bitboard::Bitboard;
use crate::hand::Hand;
use crate::movegen::MoveList;
use crate::piece::PieceType;
use crate::square::{File, Rank};
use crate::tables::{ATTACK_TABLE, BETWEEN_TABLE};
use crate::{Color, Move, Piece, Square};
use std::fmt;

#[derive(Debug)]
struct State {
    checkers: Bitboard,             // 王手をかけている駒の位置
    pinned: [Bitboard; Color::NUM], // 飛び駒から玉を守っている駒の位置
}

impl State {
    fn new(checkers: Bitboard, pinned: [Bitboard; Color::NUM]) -> Self {
        Self { checkers, pinned }
    }
    fn calculate_pinned(c_bb: &[Bitboard], pt_bb: &[Bitboard]) -> [Bitboard; Color::NUM] {
        let mut bbs = [Bitboard::ZERO, Bitboard::ZERO];
        for c in Color::ALL {
            if let Some(sq) = (c_bb[(!c).index()] & pt_bb[PieceType::OU.index()]).next() {
                #[rustfmt::skip]
                let snipers = (
                      (ATTACK_TABLE.pseudo_attack(PieceType::KY, sq, c) & pt_bb[PieceType::KY.index()])
                    | (ATTACK_TABLE.pseudo_attack(PieceType::KA, sq, c) & (pt_bb[PieceType::KA.index()] | pt_bb[PieceType::UM.index()]))
                    | (ATTACK_TABLE.pseudo_attack(PieceType::HI, sq, c) & (pt_bb[PieceType::HI.index()] | pt_bb[PieceType::RY.index()]))
                ) & c_bb[c.index()];
                for sniper in snipers {
                    let blockers = BETWEEN_TABLE[sq.index()][sniper.index()]
                        & pt_bb[PieceType::OCCUPIED.index()];
                    if blockers.count_ones() == 1 {
                        bbs[c.index()] |= blockers;
                    }
                }
            }
        }
        bbs
    }
}

/// Represents a state of the game.
#[derive(Debug)]
pub struct Position {
    board: [Piece; Square::NUM],
    hands: [Hand; Color::NUM],
    color: Color,
    c_bb: [Bitboard; Color::NUM],
    pt_bb: [Bitboard; PieceType::NUM],
    states: Vec<State>,
}

impl Position {
    pub fn new(
        board: [Piece; Square::NUM],
        hand_nums: [[u8; PieceType::NUM_HAND]; Color::NUM],
        side_to_move: Color,
    ) -> Position {
        // board
        let mut c_bb = [Bitboard::ZERO; Color::NUM];
        let mut pt_bb = [Bitboard::ZERO; PieceType::NUM];
        for sq in Square::ALL {
            let piece = board[sq.index()];
            if let Some(c) = piece.color() {
                c_bb[c.index()] |= sq;
            }
            if let Some(pt) = piece.piece_type() {
                pt_bb[PieceType::OCCUPIED.index()] |= sq;
                pt_bb[pt.index()] |= sq;
            }
        }
        // hands
        let mut hands = [Hand::new(); Color::NUM];
        for c in Color::ALL {
            for (i, &num) in hand_nums[c.index()].iter().enumerate() {
                for _ in 0..num {
                    hands[c.index()].increment(PieceType::ALL_HAND[i]);
                }
            }
        }
        // initial state
        let state = {
            let c = side_to_move;
            let checkers = Bitboard::ZERO;
            if let Some(_sq) = (c_bb[(!c).index()] & pt_bb[PieceType::OU.index()]).next() {
                // TODO
            }
            State::new(checkers, State::calculate_pinned(&c_bb, &pt_bb))
        };
        Self {
            board,
            hands,
            color: side_to_move,
            pt_bb,
            c_bb,
            states: vec![state],
        }
    }
    pub fn piece_on(&self, sq: Square) -> Piece {
        self.board[sq.index()]
    }
    pub fn pieces_cp(&self, c: Color, pt: PieceType) -> Bitboard {
        self.pieces_c(c) & self.pieces_p(pt)
    }
    pub fn pieces_c(&self, c: Color) -> Bitboard {
        self.c_bb[c.index()]
    }
    pub fn pieces_p(&self, pt: PieceType) -> Bitboard {
        self.pt_bb[pt.index()]
    }
    pub fn pieces_ps(&self, pts: &[PieceType]) -> Bitboard {
        pts.iter()
            .fold(Bitboard::ZERO, |acc, pt| acc | self.pieces_p(*pt))
    }
    pub fn occupied(&self) -> Bitboard {
        self.pt_bb[PieceType::OCCUPIED.index()]
    }
    pub fn in_check(&self) -> bool {
        !self.checkers().is_empty()
    }
    pub fn checkers(&self) -> Bitboard {
        self.state().expect("empty states").checkers
    }
    pub fn pinned(&self) -> [Bitboard; Color::NUM] {
        self.state().expect("empty states").pinned
    }
    pub fn king(&self, c: Color) -> Option<Square> {
        self.pieces_cp(c, PieceType::OU).next()
    }
    pub fn hand(&self, c: Color) -> Hand {
        self.hands[c.index()]
    }
    pub fn side_to_move(&self) -> Color {
        self.color
    }
    pub fn legal_moves(&self) -> MoveList {
        let mut ml = MoveList::default();
        ml.generate_legals(self);
        ml
    }
    pub fn do_move(&mut self, m: Move) {
        let state = if let Some(from) = m.from() {
            self.do_normal_move(from, m.to(), m.is_promotion())
        } else {
            self.do_drop_move(m.to(), m.piece())
        };
        self.states.push(state);
        self.color = !self.color;
    }
    pub fn undo_move(&mut self, m: Move) {
        let c = self.side_to_move();
        let to = m.to();
        let p_to = self.piece_on(to);
        // 駒移動
        if let Some(from) = m.from() {
            self.remove_piece(to, p_to);
            if let Some(p_cap) = m.captured() {
                self.put_piece(to, p_cap);
                if let Some(pt) = p_cap.piece_type() {
                    self.hands[(!c).index()].decrement(pt);
                }
            }
            let p_from = if m.is_promotion() {
                p_to.demoted()
            } else {
                p_to
            };
            self.put_piece(from, p_from);
        }
        // 駒打ち
        else {
            self.remove_piece(to, p_to);
            if let Some(pt) = p_to.piece_type() {
                self.hands[(!c).index()].increment(pt);
            }
        }
        self.color = !self.color;
        self.states.pop();
    }
    // 駒移動
    fn do_normal_move(&mut self, from: Square, to: Square, promotion: bool) -> State {
        let c = self.side_to_move();
        let p_from = self.piece_on(from);
        self.remove_piece(from, p_from);
        // 移動先に駒がある場合
        if let Some(pt) = self.piece_on(to).piece_type() {
            self.xor_bbs(!c, pt, to);
            self.hands[c.index()].increment(pt);
        }
        let p_to = if promotion { p_from.promoted() } else { p_from };
        self.put_piece(to, p_to);
        let checkers = if let Some(sq) = self.king(!c) {
            self.attackers_to(c, sq)
        } else {
            Bitboard::ZERO
        };
        State::new(checkers, State::calculate_pinned(&self.c_bb, &self.pt_bb))
    }
    // 駒打ち
    fn do_drop_move(&mut self, to: Square, p: Piece) -> State {
        let c = self.side_to_move();
        let pt = p.piece_type().unwrap();
        self.put_piece(to, p);
        self.hands[c.index()].decrement(pt);
        let checkers = if self.king(!c).map_or(false, |sq| {
            !(ATTACK_TABLE.attack(pt, to, c, &self.occupied()) & sq).is_empty()
        }) {
            Bitboard::from_square(to)
        } else {
            Bitboard::ZERO
        };
        State::new(checkers, State::calculate_pinned(&self.c_bb, &self.pt_bb))
    }
    fn state(&self) -> Option<&State> {
        self.states.last()
    }
    fn put_piece(&mut self, sq: Square, p: Piece) {
        if let (Some(c), Some(pt)) = (p.color(), p.piece_type()) {
            self.xor_bbs(c, pt, sq);
        } else {
            panic!("failed to put piece: square: {:?}, piece: {:?}", sq, p);
        }
        self.board[sq.index()] = p;
    }
    fn remove_piece(&mut self, sq: Square, p: Piece) {
        if let (Some(c), Some(pt)) = (p.color(), p.piece_type()) {
            self.xor_bbs(c, pt, sq);
        } else {
            panic!("failed to remove piece: square: {:?}, piece: {:?}", sq, p);
        }
        self.board[sq.index()] = Piece::EMP;
    }
    fn xor_bbs(&mut self, c: Color, pt: PieceType, sq: Square) {
        self.c_bb[c.index()] ^= sq;
        self.pt_bb[PieceType::OCCUPIED.index()] ^= sq;
        self.pt_bb[pt.index()] ^= sq;
    }
    #[rustfmt::skip]
    pub fn attackers_to(&self, c: Color, to: Square) -> Bitboard {
        let opp = !c;
        let occ = self.occupied();
        (     (ATTACK_TABLE.attack(PieceType::FU, to, opp, &occ) & self.pieces_p(PieceType::FU))
            | (ATTACK_TABLE.attack(PieceType::KY, to, opp, &occ) & self.pieces_p(PieceType::KY))
            | (ATTACK_TABLE.attack(PieceType::KE, to, opp, &occ) & self.pieces_p(PieceType::KE))
            | (ATTACK_TABLE.attack(PieceType::GI, to, opp, &occ) & self.pieces_ps(&[PieceType::GI, PieceType::RY, PieceType::OU]))
            | (ATTACK_TABLE.attack(PieceType::KA, to, opp, &occ) & self.pieces_ps(&[PieceType::KA, PieceType::UM]))
            | (ATTACK_TABLE.attack(PieceType::HI, to, opp, &occ) & self.pieces_ps(&[PieceType::HI, PieceType::RY]))
            | (ATTACK_TABLE.attack(PieceType::KI, to, opp, &occ) & self.pieces_ps(&[PieceType::KI, PieceType::TO, PieceType::NY, PieceType::NK, PieceType::NG, PieceType::UM, PieceType::OU]))
        ) & self.pieces_c(c)
    }
}

impl Default for Position {
    fn default() -> Self {
        #[rustfmt::skip]
        let board = [
            Piece::WKY, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BKY,
            Piece::WKE, Piece::WKA, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::BHI, Piece::BKE,
            Piece::WGI, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BGI,
            Piece::WKI, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BKI,
            Piece::WOU, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BOU,
            Piece::WKI, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BKI,
            Piece::WGI, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BGI,
            Piece::WKE, Piece::WHI, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::BKA, Piece::BKE,
            Piece::WKY, Piece::EMP, Piece::WFU, Piece::EMP, Piece::EMP, Piece::EMP, Piece::BFU, Piece::EMP, Piece::BKY,
        ];
        Self::new(board, [[0; 7]; 2], Color::Black)
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &rank in Rank::ALL.iter() {
            write!(f, "P{}", rank.0 + 1)?;
            for &file in File::ALL.iter().rev() {
                write!(f, "{}", self.piece_on(Square::new(file, rank)))?;
            }
            writeln!(f)?;
        }
        for c in Color::ALL {
            if !self.hands[c.index()].is_empty() {
                write!(
                    f,
                    "P{}",
                    match c {
                        Color::Black => "+",
                        Color::White => "-",
                    }
                )?;
                for &pt in PieceType::ALL_HAND.iter().rev() {
                    for _ in 0..self.hands[c.index()].num(pt) {
                        write!(f, "00{}", pt)?;
                    }
                }
                writeln!(f)?;
            }
        }
        writeln!(
            f,
            "{}",
            match self.side_to_move() {
                Color::Black => "+",
                Color::White => "-",
            }
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let pos = Position::default();
        for sq in Square::ALL {
            #[rustfmt::skip]
            let expected = match sq {
                Square::SQ17 | Square::SQ27 | Square::SQ37 | Square::SQ47 | Square::SQ57 | Square::SQ67 | Square::SQ77 | Square::SQ87 | Square::SQ97 => Piece::BFU,
                Square::SQ19 | Square::SQ99 => Piece::BKY,
                Square::SQ29 | Square::SQ89 => Piece::BKE,
                Square::SQ39 | Square::SQ79 => Piece::BGI,
                Square::SQ49 | Square::SQ69 => Piece::BKI,
                Square::SQ59 => Piece::BOU,
                Square::SQ88 => Piece::BKA,
                Square::SQ28 => Piece::BHI,
                Square::SQ13 | Square::SQ23 | Square::SQ33 | Square::SQ43 | Square::SQ53 | Square::SQ63 | Square::SQ73 | Square::SQ83 | Square::SQ93 => Piece::WFU,
                Square::SQ11 | Square::SQ91 => Piece::WKY,
                Square::SQ21 | Square::SQ81 => Piece::WKE,
                Square::SQ31 | Square::SQ71 => Piece::WGI,
                Square::SQ41 | Square::SQ61 => Piece::WKI,
                Square::SQ51 => Piece::WOU,
                Square::SQ22 => Piece::WKA,
                Square::SQ82 => Piece::WHI,
                _ => Piece::EMP,
            };
            assert_eq!(expected, pos.piece_on(sq), "square: {:?}", sq);
        }
        for c in Color::ALL {
            for pt in PieceType::ALL_HAND {
                assert_eq!(0, pos.hand(c).num(pt));
            }
        }
        assert_eq!(Color::Black, pos.side_to_move());
        assert!(!pos.in_check());
    }

    #[test]
    fn test_do_undo_move() {
        let mut pos = Position::default();
        let moves = [
            Move::new_normal(Square::SQ77, Square::SQ76, false, Piece::BFU, Piece::EMP),
            Move::new_normal(Square::SQ33, Square::SQ34, false, Piece::WFU, Piece::EMP),
            Move::new_normal(Square::SQ88, Square::SQ22, true, Piece::BUM, Piece::WKA),
            Move::new_normal(Square::SQ31, Square::SQ22, false, Piece::WGI, Piece::BUM),
            Move::new_drop(Square::SQ33, Piece::BKA),
        ];
        // do moves
        for &m in moves.iter() {
            pos.do_move(m);
        }
        // check moved pieces, position states
        for (sq, expected) in [
            (Square::SQ22, Piece::WGI),
            (Square::SQ31, Piece::EMP),
            (Square::SQ33, Piece::BKA),
            (Square::SQ76, Piece::BFU),
            (Square::SQ77, Piece::EMP),
        ] {
            assert_eq!(expected, pos.piece_on(sq), "square: {:?}", sq);
        }
        assert!(pos.hand(Color::Black).is_empty());
        assert!(!pos.hand(Color::White).is_empty());
        assert_eq!(Color::White, pos.side_to_move());
        assert!(pos.in_check());
        // revert to default position
        for &m in moves.iter().rev() {
            pos.undo_move(m);
        }
        let default = Position::default();
        assert!(Square::ALL
            .iter()
            .all(|&sq| pos.piece_on(sq) == default.piece_on(sq)));
        assert_eq!(Color::Black, pos.side_to_move());
        assert!(!pos.in_check());
    }
}
