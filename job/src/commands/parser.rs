use elyze::acceptor::Acceptor;
use elyze::bytes::components::groups::GroupKind;
use elyze::bytes::matchers::match_pattern;
use elyze::bytes::primitives::whitespace::{OptionalWhitespaces, Whitespaces};
use elyze::bytes::token::Token;
use elyze::errors::ParseError::UnexpectedToken;
use elyze::errors::ParseResult;
use elyze::matcher::Match;
use elyze::peek::{UntilEnd, peek};
use elyze::peeker::Peeker;
use elyze::recognizer::recognize;
use elyze::scanner::Scanner;
use elyze::visitor::Visitor;

enum Keyword {
    Register,
    Unregister,
}

impl Match<u8> for Keyword {
    fn is_matching(&self, data: &[u8]) -> (bool, usize) {
        match self {
            Keyword::Register => match_pattern(b"register", data),
            Keyword::Unregister => match_pattern(b"unregister", data),
        }
    }

    fn size(&self) -> usize {
        match self {
            Keyword::Register => "register".len(),
            Keyword::Unregister => "unregister".len(),
        }
    }
}

struct Data<'a> {
    value: &'a str,
}

impl<'a> Visitor<'a, u8> for Data<'a> {
    fn accept(scanner: &mut Scanner<'a, u8>) -> ParseResult<Self> {
        OptionalWhitespaces::accept(scanner)?;
        let peeked = peek(GroupKind::DoubleQuotes, scanner)?.ok_or(UnexpectedToken)?;
        let value = std::str::from_utf8(peeked.peeked_slice())?;
        scanner.bump_by(peeked.end_slice);
        Ok(Self { value })
    }
}

struct Number {
    value: i64,
}

impl<'a> Visitor<'a, u8> for Number {
    fn accept(scanner: &mut Scanner<'a, u8>) -> ParseResult<Self> {
        let data = Peeker::new(scanner)
            .add_peekable(Token::Whitespace)
            .add_peekable(UntilEnd::default())
            .peek()?
            .ok_or(UnexpectedToken)?;
        let value = std::str::from_utf8(data.peeked_slice())?.parse::<i64>()?;
        scanner.bump_by(data.peeked_slice().len());
        Ok(Self { value })
    }
}

#[derive(Debug, PartialEq)]
pub struct CommandRegister<'a> {
    pub tenant: &'a str,
    pub executor_id: i64,
    pub interest: &'a str,
}

impl<'a> Visitor<'a, u8> for CommandRegister<'a> {
    fn accept(scanner: &mut Scanner<'a, u8>) -> ParseResult<Self> {
        recognize(Keyword::Register, scanner)?;
        Whitespaces::accept(scanner)?;
        let tenant = Data::accept(scanner)?.value;
        Whitespaces::accept(scanner)?;
        let executor_id = Number::accept(scanner)?.value;
        Whitespaces::accept(scanner)?;
        let interest = Data::accept(scanner)?.value;
        OptionalWhitespaces::accept(scanner)?;
        Ok(Self {
            tenant,
            executor_id,
            interest,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct CommandUnregister<'a> {
    pub tenant: &'a str,
    pub executor_id: i64,
    pub watch_id: i64,
}

impl<'a> Visitor<'a, u8> for CommandUnregister<'a> {
    fn accept(scanner: &mut Scanner<'a, u8>) -> ParseResult<Self> {
        recognize(Keyword::Unregister, scanner)?;
        Whitespaces::accept(scanner)?;
        let tenant = Data::accept(scanner)?.value;
        Whitespaces::accept(scanner)?;
        let executor_id = Number::accept(scanner)?.value;
        Whitespaces::accept(scanner)?;
        let watch_id = Number::accept(scanner)?.value;
        OptionalWhitespaces::accept(scanner)?;
        Ok(CommandUnregister {
            tenant,
            executor_id,
            watch_id,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum Command<'a> {
    Register(CommandRegister<'a>),
    Unregister(CommandUnregister<'a>),
}

impl<'a> Command<'a> {
    pub fn from_slice(data: &'a [u8]) -> ParseResult<Self> {
        let mut scanner = Scanner::new(data);
        Command::accept(&mut scanner)
    }
}

impl<'a> Visitor<'a, u8> for Command<'a> {
    fn accept(scanner: &mut Scanner<'a, u8>) -> ParseResult<Self> {
        Ok(Acceptor::new(scanner)
            .try_or(Command::Register)?
            .try_or(Command::Unregister)?
            .finish()
            .ok_or(UnexpectedToken)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::parser::{Command, CommandRegister, CommandUnregister};
    use elyze::scanner::Scanner;
    use elyze::visitor::Visitor;

    #[test]
    fn test_command_register() {
        let data = br#"register "tenant1" 12 "x > 3""#;
        let mut scanner = Scanner::new(data);
        let command = Command::accept(&mut scanner).expect("Should parse register command");
        assert_eq!(
            command,
            Command::Register(CommandRegister {
                executor_id: 12,
                tenant: "tenant1",
                interest: "x > 3",
            })
        )
    }

    #[test]
    fn test_command_unregister() {
        let data = br#"unregister "tenant 1" 12 666"#;
        let mut scanner = Scanner::new(data);
        let command = Command::accept(&mut scanner).expect("Should parse register command");
        assert_eq!(
            command,
            Command::Unregister(CommandUnregister {
                executor_id: 12,
                tenant: "tenant 1",
                watch_id: 666
            })
        )
    }
}
